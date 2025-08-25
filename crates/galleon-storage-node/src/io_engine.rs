use anyhow::{Result, anyhow, Context};
use aligned_vec::AlignedVec;
use crossbeam::channel::{self, Receiver, Sender};
use dashmap::DashMap;
use io_uring::{IoUring, opcode, types, SubmissionQueue, CompletionQueue, Probe};
use libc::{c_void, sysconf, _SC_PAGESIZE};
use nix::sys::mman::{mmap, munmap, MapFlags, ProtFlags};
use nix::unistd::sysconf as nix_sysconf;
use nix::unistd::SysconfVar;
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{self, IoSlice, IoSliceMut};
use std::os::unix::io::{AsRawFd, RawFd};
use std::ptr::NonNull;
use std::sync::{Arc, atomic::{AtomicU64, AtomicUsize, Ordering}};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tracing::{debug, info, warn, error, trace};
use uuid::Uuid;

use galleon_common::types::{DeviceId, ChunkId};
use galleon_common::error::StorageError;
use crate::device_manager::{ClaimedDevice, PerformanceClass};

/// I/O operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoOpType {
    Read,
    Write,
    Sync,
    Discard,
}

/// I/O operation request
#[derive(Debug)]
pub struct IoRequest {
    pub request_id: Uuid,
    pub device_id: DeviceId,
    pub op_type: IoOpType,
    pub offset: u64,
    pub data: IoBuffer,
    pub completion_tx: oneshot::Sender<Result<IoResponse>>,
    pub priority: IoPriority,
    pub numa_node: Option<u32>,
    pub submitted_at: Instant,
}

/// I/O operation response
#[derive(Debug)]
pub struct IoResponse {
    pub request_id: Uuid,
    pub bytes_transferred: usize,
    pub latency: Duration,
    pub completed_at: Instant,
}

/// I/O priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IoPriority {
    Critical = 0,    // Metadata, control operations
    High = 1,        // User data with low latency requirements
    Normal = 2,      // Standard user data
    Low = 3,         // Background operations, garbage collection
    Batch = 4,       // Bulk operations
}

/// Aligned I/O buffer for O_DIRECT operations
#[derive(Debug)]
pub struct IoBuffer {
    data: AlignedVec<u8>,
    capacity: usize,
    alignment: usize,
}

impl IoBuffer {
    /// Create a new aligned I/O buffer
    pub fn new(size: usize, alignment: usize) -> Result<Self> {
        let mut data = AlignedVec::new(alignment);
        data.resize(size, 0);
        
        Ok(Self {
            data,
            capacity: size,
            alignment,
        })
    }
    
    /// Create buffer from existing data
    pub fn from_data(data: &[u8], alignment: usize) -> Result<Self> {
        let mut buffer = Self::new(data.len(), alignment)?;
        buffer.data.copy_from_slice(data);
        Ok(buffer)
    }
    
    /// Get buffer size
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// Get buffer data as slice
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
    
    /// Get mutable buffer data
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
    
    /// Get buffer pointer for io_uring
    pub fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }
    
    /// Get mutable buffer pointer for io_uring
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }
}

/// Buffer pool for pre-allocated, aligned buffers
pub struct BufferPool {
    /// Available buffers by size
    pools: HashMap<usize, VecDeque<IoBuffer>>,
    /// Total allocated buffers
    total_buffers: AtomicUsize,
    /// Maximum buffers per size
    max_buffers_per_size: usize,
    /// Buffer alignment
    alignment: usize,
    /// NUMA node preference
    numa_node: Option<u32>,
}

impl BufferPool {
    /// Create a new buffer pool
    pub fn new(max_buffers_per_size: usize, alignment: usize, numa_node: Option<u32>) -> Self {
        Self {
            pools: HashMap::new(),
            total_buffers: AtomicUsize::new(0),
            max_buffers_per_size,
            alignment,
            numa_node,
        }
    }
    
    /// Get buffer from pool or allocate new one
    pub fn get_buffer(&mut self, size: usize) -> Result<IoBuffer> {
        if let Some(pool) = self.pools.get_mut(&size) {
            if let Some(buffer) = pool.pop_front() {
                return Ok(buffer);
            }
        }
        
        // Allocate new buffer
        let buffer = IoBuffer::new(size, self.alignment)?;
        self.total_buffers.fetch_add(1, Ordering::Relaxed);
        Ok(buffer)
    }
    
    /// Return buffer to pool
    pub fn return_buffer(&mut self, mut buffer: IoBuffer) {
        let size = buffer.len();
        
        // Clear buffer data for security
        buffer.as_mut_slice().fill(0);
        
        let pool = self.pools.entry(size).or_insert_with(VecDeque::new);
        
        if pool.len() < self.max_buffers_per_size {
            pool.push_back(buffer);
        } else {
            // Drop buffer if pool is full
            self.total_buffers.fetch_sub(1, Ordering::Relaxed);
        }
    }
    
    /// Get pool statistics
    pub fn get_stats(&self) -> BufferPoolStats {
        let mut total_pooled = 0;
        let mut pools_by_size = HashMap::new();
        
        for (size, pool) in &self.pools {
            let count = pool.len();
            total_pooled += count;
            pools_by_size.insert(*size, count);
        }
        
        BufferPoolStats {
            total_allocated: self.total_buffers.load(Ordering::Relaxed),
            total_pooled,
            pools_by_size,
        }
    }
}

/// Buffer pool statistics
#[derive(Debug, Clone)]
pub struct BufferPoolStats {
    pub total_allocated: usize,
    pub total_pooled: usize,
    pub pools_by_size: HashMap<usize, usize>,
}

/// io_uring ring for a specific device
pub struct IoRing {
    ring: IoUring,
    device_fd: RawFd,
    device_id: DeviceId,
    numa_node: Option<u32>,
    
    /// Pending requests waiting for submission
    pending_queue: VecDeque<IoRequest>,
    
    /// In-flight requests
    in_flight: HashMap<u64, IoRequest>,
    
    /// Next user data ID
    next_user_data: AtomicU64,
    
    /// Ring statistics
    stats: IoRingStats,
    
    /// Ring configuration
    config: IoRingConfig,
}

/// io_uring ring statistics
#[derive(Debug, Clone, Default)]
pub struct IoRingStats {
    pub submitted_ops: AtomicU64,
    pub completed_ops: AtomicU64,
    pub failed_ops: AtomicU64,
    pub queue_depth: AtomicUsize,
    pub avg_latency_us: AtomicU64,
    pub total_bytes: AtomicU64,
}

/// io_uring ring configuration
#[derive(Debug, Clone)]
pub struct IoRingConfig {
    pub queue_depth: u32,
    pub batch_size: u32,
    pub enable_polling: bool,
    pub enable_sqpoll: bool,
    pub sqpoll_cpu: Option<u32>,
    pub cq_size_multiplier: u32,
}

impl Default for IoRingConfig {
    fn default() -> Self {
        Self {
            queue_depth: 256,
            batch_size: 32,
            enable_polling: false,
            enable_sqpoll: false,
            sqpoll_cpu: None,
            cq_size_multiplier: 2,
        }
    }
}

impl IoRing {
    /// Create a new io_uring ring for a device
    pub fn new(
        device: &ClaimedDevice,
        config: IoRingConfig,
    ) -> Result<Self> {
        // Open device file
        let device_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_DIRECT)
            .open(&device.device_path)
            .context("Failed to open device")?;
        
        let device_fd = device_file.as_raw_fd();
        
        // Create io_uring with configuration
        let mut ring_builder = IoUring::builder();
        ring_builder.setup_cqsize(config.queue_depth * config.cq_size_multiplier);
        
        if config.enable_sqpoll {
            ring_builder.setup_sqpoll(1000); // 1 second idle timeout
            if let Some(cpu) = config.sqpoll_cpu {
                ring_builder.setup_sqpoll_cpu(cpu);
            }
        }
        
        let ring = ring_builder.build(config.queue_depth)
            .context("Failed to create io_uring")?;
        
        // Keep device file open
        std::mem::forget(device_file);
        
        info!("Created io_uring for device {} with queue depth {}", 
              device.device_id, config.queue_depth);
        
        Ok(Self {
            ring,
            device_fd,
            device_id: device.device_id.clone(),
            numa_node: device.numa_node,
            pending_queue: VecDeque::new(),
            in_flight: HashMap::new(),
            next_user_data: AtomicU64::new(1),
            stats: IoRingStats::default(),
            config,
        })
    }
    
    /// Submit I/O request
    pub fn submit_request(&mut self, request: IoRequest) -> Result<()> {
        self.pending_queue.push_back(request);
        self.try_submit_pending()?;
        Ok(())
    }
    
    /// Try to submit pending requests
    fn try_submit_pending(&mut self) -> Result<()> {
        let mut submitted = 0;
        
        while submitted < self.config.batch_size && !self.pending_queue.is_empty() {
            let mut sq = self.ring.submission();
            
            if sq.is_full() {
                break;
            }
            
            if let Some(request) = self.pending_queue.pop_front() {
                self.submit_single_request(&mut sq, request)?;
                submitted += 1;
            }
        }
        
        if submitted > 0 {
            let submitted_count = self.ring.submit()?;
            self.stats.submitted_ops.fetch_add(submitted_count as u64, Ordering::Relaxed);
            debug!("Submitted {} I/O operations for device {}", 
                   submitted_count, self.device_id);
        }
        
        Ok(())
    }
    
    /// Submit a single request to the submission queue
    fn submit_single_request(
        &mut self,
        sq: &mut SubmissionQueue,
        request: IoRequest,
    ) -> Result<()> {
        let user_data = self.next_user_data.fetch_add(1, Ordering::Relaxed);
        let fd = types::Fd(self.device_fd);
        
        let sqe = match request.op_type {
            IoOpType::Read => {
                opcode::Read::new(fd, request.data.as_mut_ptr(), request.data.len() as u32)
                    .offset(request.offset)
                    .build()
            }
            IoOpType::Write => {
                opcode::Write::new(fd, request.data.as_ptr(), request.data.len() as u32)
                    .offset(request.offset)
                    .build()
            }
            IoOpType::Sync => {
                opcode::Fsync::new(fd)
                    .build()
            }
            IoOpType::Discard => {
                // Use fallocate with PUNCH_HOLE
                opcode::Fallocate::new(
                    fd,
                    request.offset as i64,
                    request.data.len() as i64,
                )
                .mode(libc::FALLOC_FL_PUNCH_HOLE | libc::FALLOC_FL_KEEP_SIZE)
                .build()
            }
        };
        
        // Set user data and priority
        let mut sqe = sqe.user_data(user_data);
        
        // Set I/O priority if supported
        if request.priority != IoPriority::Normal {
            sqe = sqe.ioprio(request.priority as u16);
        }
        
        // Add to submission queue
        unsafe {
            sq.push(&sqe)
                .map_err(|_| anyhow!("Failed to push SQE"))?;
        }
        
        // Store request for completion handling
        self.in_flight.insert(user_data, request);
        self.stats.queue_depth.fetch_add(1, Ordering::Relaxed);
        
        Ok(())
    }
    
    /// Process completion queue and handle completed requests
    pub fn process_completions(&mut self) -> Result<usize> {
        let mut completed = 0;
        let mut cq = self.ring.completion();
        
        while let Some(cqe) = cq.next() {
            let user_data = cqe.user_data();
            let result = cqe.result();
            
            if let Some(request) = self.in_flight.remove(&user_data) {
                let latency = request.submitted_at.elapsed();
                
                // Update statistics
                self.stats.completed_ops.fetch_add(1, Ordering::Relaxed);
                self.stats.queue_depth.fetch_sub(1, Ordering::Relaxed);
                
                // Update average latency
                let latency_us = latency.as_micros() as u64;
                self.update_avg_latency(latency_us);
                
                let response = if result >= 0 {
                    self.stats.total_bytes.fetch_add(result as u64, Ordering::Relaxed);
                    
                    Ok(IoResponse {
                        request_id: request.request_id,
                        bytes_transferred: result as usize,
                        latency,
                        completed_at: Instant::now(),
                    })
                } else {
                    self.stats.failed_ops.fetch_add(1, Ordering::Relaxed);
                    Err(anyhow!("I/O operation failed with error: {}", -result))
                };
                
                // Send completion to waiting task
                if let Err(_) = request.completion_tx.send(response) {
                    debug!("Failed to send completion for request {}", request.request_id);
                }
                
                completed += 1;
            } else {
                warn!("Received completion for unknown user_data: {}", user_data);
            }
        }
        
        if completed > 0 {
            trace!("Processed {} completions for device {}", completed, self.device_id);
        }
        
        Ok(completed)
    }
    
    /// Update running average latency
    fn update_avg_latency(&self, latency_us: u64) {
        // Simple exponential moving average
        let current_avg = self.stats.avg_latency_us.load(Ordering::Relaxed);
        let new_avg = if current_avg == 0 {
            latency_us
        } else {
            // EMA with alpha = 0.1
            (current_avg * 9 + latency_us) / 10
        };
        self.stats.avg_latency_us.store(new_avg, Ordering::Relaxed);
    }
    
    /// Get ring statistics
    pub fn get_stats(&self) -> IoRingStats {
        IoRingStats {
            submitted_ops: AtomicU64::new(self.stats.submitted_ops.load(Ordering::Relaxed)),
            completed_ops: AtomicU64::new(self.stats.completed_ops.load(Ordering::Relaxed)),
            failed_ops: AtomicU64::new(self.stats.failed_ops.load(Ordering::Relaxed)),
            queue_depth: AtomicUsize::new(self.stats.queue_depth.load(Ordering::Relaxed)),
            avg_latency_us: AtomicU64::new(self.stats.avg_latency_us.load(Ordering::Relaxed)),
            total_bytes: AtomicU64::new(self.stats.total_bytes.load(Ordering::Relaxed)),
        }
    }
    
    /// Get current queue depth
    pub fn queue_depth(&self) -> usize {
        self.stats.queue_depth.load(Ordering::Relaxed)
    }
    
    /// Check if ring has capacity for more requests
    pub fn has_capacity(&self) -> bool {
        self.queue_depth() < (self.config.queue_depth as usize * 3 / 4)
    }
}

/// High-performance I/O engine with multi-ring architecture
pub struct IoEngine {
    /// Map of device ID to io_uring ring
    rings: Arc<DashMap<DeviceId, Arc<Mutex<IoRing>>>>,
    
    /// Buffer pools per NUMA node
    buffer_pools: Arc<DashMap<u32, Arc<Mutex<BufferPool>>>>,
    
    /// Request channel for load balancing
    request_tx: mpsc::UnboundedSender<IoRequest>,
    request_rx: Arc<Mutex<mpsc::UnboundedReceiver<IoRequest>>>,
    
    /// Worker thread handles
    worker_handles: Vec<tokio::task::JoinHandle<()>>,
    
    /// Engine configuration
    config: IoEngineConfig,
    
    /// Engine statistics
    stats: Arc<IoEngineStats>,
    
    /// Shutdown signal
    shutdown_tx: Option<oneshot::Sender<()>>,
}

/// I/O engine configuration
#[derive(Debug, Clone)]
pub struct IoEngineConfig {
    pub workers_per_numa_node: usize,
    pub max_queue_depth_per_device: u32,
    pub buffer_pool_size: usize,
    pub enable_cpu_affinity: bool,
    pub enable_numa_optimization: bool,
    pub enable_polling: bool,
    pub polling_interval_us: u64,
    pub batch_completion_size: usize,
}

impl Default for IoEngineConfig {
    fn default() -> Self {
        Self {
            workers_per_numa_node: 2,
            max_queue_depth_per_device: 256,
            buffer_pool_size: 1000,
            enable_cpu_affinity: true,
            enable_numa_optimization: true,
            enable_polling: false,
            polling_interval_us: 100,
            batch_completion_size: 32,
        }
    }
}

/// I/O engine statistics
#[derive(Debug, Default)]
pub struct IoEngineStats {
    pub total_requests: AtomicU64,
    pub completed_requests: AtomicU64,
    pub failed_requests: AtomicU64,
    pub total_bytes_read: AtomicU64,
    pub total_bytes_written: AtomicU64,
    pub avg_latency_us: AtomicU64,
    pub active_devices: AtomicUsize,
    pub worker_threads: AtomicUsize,
}

impl IoEngine {
    /// Create a new I/O engine
    pub fn new(config: IoEngineConfig) -> Result<Self> {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        
        let engine = Self {
            rings: Arc::new(DashMap::new()),
            buffer_pools: Arc::new(DashMap::new()),
            request_tx,
            request_rx: Arc::new(Mutex::new(request_rx)),
            worker_handles: Vec::new(),
            config,
            stats: Arc::new(IoEngineStats::default()),
            shutdown_tx: None,
        };
        
        info!("Created I/O engine with configuration: {:?}", engine.config);
        Ok(engine)
    }
    
    /// Add a device to the I/O engine
    pub fn add_device(&mut self, device: &ClaimedDevice) -> Result<()> {
        let ring_config = IoRingConfig {
            queue_depth: self.config.max_queue_depth_per_device,
            batch_size: 32,
            enable_polling: self.config.enable_polling,
            enable_sqpoll: false, // Disable SQPOLL for better control
            sqpoll_cpu: None,
            cq_size_multiplier: 2,
        };
        
        let ring = IoRing::new(device, ring_config)?;
        self.rings.insert(device.device_id.clone(), Arc::new(Mutex::new(ring)));
        
        // Create buffer pool for device's NUMA node if needed
        if let Some(numa_node) = device.numa_node {
            if !self.buffer_pools.contains_key(&numa_node) {
                let buffer_pool = BufferPool::new(
                    self.config.buffer_pool_size,
                    4096, // 4KB alignment for O_DIRECT
                    Some(numa_node),
                );
                self.buffer_pools.insert(numa_node, Arc::new(Mutex::new(buffer_pool)));
            }
        }
        
        self.stats.active_devices.fetch_add(1, Ordering::Relaxed);
        info!("Added device {} to I/O engine", device.device_id);
        Ok(())
    }
    
    /// Remove a device from the I/O engine
    pub fn remove_device(&mut self, device_id: &DeviceId) -> Result<()> {
        if let Some((_, _)) = self.rings.remove(device_id) {
            self.stats.active_devices.fetch_sub(1, Ordering::Relaxed);
            info!("Removed device {} from I/O engine", device_id);
        }
        Ok(())
    }
    
    /// Start I/O workers
    pub fn start_workers(&mut self, numa_topology: &HashMap<u32, Vec<u32>>) -> Result<()> {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);
        
        for (numa_node, cpus) in numa_topology {
            for worker_id in 0..self.config.workers_per_numa_node {
                let worker = IoWorker::new(
                    format!("io-worker-{}-{}", numa_node, worker_id),
                    *numa_node,
                    cpus.clone(),
                    self.rings.clone(),
                    self.buffer_pools.clone(),
                    self.request_rx.clone(),
                    self.stats.clone(),
                    self.config.clone(),
                    shutdown_rx.try_clone().unwrap(),
                );
                
                let handle = tokio::spawn(async move {
                    worker.run().await;
                });
                
                self.worker_handles.push(handle);
                self.stats.worker_threads.fetch_add(1, Ordering::Relaxed);
            }
        }
        
        info!("Started {} I/O workers across {} NUMA nodes", 
              self.worker_handles.len(), numa_topology.len());
        Ok(())
    }
    
    /// Submit I/O request
    pub async fn submit_io(&self, request: IoRequest) -> Result<IoResponse> {
        let (completion_tx, completion_rx) = oneshot::channel();
        
        let request_with_completion = IoRequest {
            completion_tx,
            ..request
        };
        
        self.request_tx.send(request_with_completion)
            .map_err(|_| anyhow!("Failed to submit I/O request"))?;
        
        self.stats.total_requests.fetch_add(1, Ordering::Relaxed);
        
        completion_rx.await
            .map_err(|_| anyhow!("I/O request was dropped"))?
    }
    
    /// Create aligned buffer for I/O
    pub fn create_buffer(&self, size: usize, numa_node: Option<u32>) -> Result<IoBuffer> {
        let numa_node = numa_node.unwrap_or(0);
        
        if let Some(pool) = self.buffer_pools.get(&numa_node) {
            pool.lock().get_buffer(size)
        } else {
            IoBuffer::new(size, 4096) // 4KB alignment
        }
    }
    
    /// Return buffer to pool
    pub fn return_buffer(&self, buffer: IoBuffer, numa_node: Option<u32>) {
        let numa_node = numa_node.unwrap_or(0);
        
        if let Some(pool) = self.buffer_pools.get(&numa_node) {
            pool.lock().return_buffer(buffer);
        }
        // If no pool exists, buffer will be dropped
    }
    
    /// Get engine statistics
    pub fn get_stats(&self) -> IoEngineStats {
        IoEngineStats {
            total_requests: AtomicU64::new(self.stats.total_requests.load(Ordering::Relaxed)),
            completed_requests: AtomicU64::new(self.stats.completed_requests.load(Ordering::Relaxed)),
            failed_requests: AtomicU64::new(self.stats.failed_requests.load(Ordering::Relaxed)),
            total_bytes_read: AtomicU64::new(self.stats.total_bytes_read.load(Ordering::Relaxed)),
            total_bytes_written: AtomicU64::new(self.stats.total_bytes_written.load(Ordering::Relaxed)),
            avg_latency_us: AtomicU64::new(self.stats.avg_latency_us.load(Ordering::Relaxed)),
            active_devices: AtomicUsize::new(self.stats.active_devices.load(Ordering::Relaxed)),
            worker_threads: AtomicUsize::new(self.stats.worker_threads.load(Ordering::Relaxed)),
        }
    }
    
    /// Shutdown the I/O engine
    pub async fn shutdown(mut self) -> Result<()> {
        info!("Shutting down I/O engine");
        
        // Send shutdown signal
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        
        // Wait for workers to finish
        for handle in self.worker_handles {
            if let Err(e) = handle.await {
                warn!("Worker thread failed to shutdown cleanly: {}", e);
            }
        }
        
        info!("I/O engine shutdown complete");
        Ok(())
    }
}

/// I/O worker thread
struct IoWorker {
    name: String,
    numa_node: u32,
    cpus: Vec<u32>,
    rings: Arc<DashMap<DeviceId, Arc<Mutex<IoRing>>>>,
    buffer_pools: Arc<DashMap<u32, Arc<Mutex<BufferPool>>>>,
    request_rx: Arc<Mutex<mpsc::UnboundedReceiver<IoRequest>>>,
    stats: Arc<IoEngineStats>,
    config: IoEngineConfig,
    shutdown_rx: oneshot::Receiver<()>,
}

impl IoWorker {
    fn new(
        name: String,
        numa_node: u32,
        cpus: Vec<u32>,
        rings: Arc<DashMap<DeviceId, Arc<Mutex<IoRing>>>>,
        buffer_pools: Arc<DashMap<u32, Arc<Mutex<BufferPool>>>>,
        request_rx: Arc<Mutex<mpsc::UnboundedReceiver<IoRequest>>>,
        stats: Arc<IoEngineStats>,
        config: IoEngineConfig,
        shutdown_rx: oneshot::Receiver<()>,
    ) -> Self {
        Self {
            name,
            numa_node,
            cpus,
            rings,
            buffer_pools,
            request_rx,
            stats,
            config,
            shutdown_rx,
        }
    }
    
    async fn run(mut self) {
        info!("Starting I/O worker: {}", self.name);
        
        // Set CPU affinity if enabled
        if self.config.enable_cpu_affinity && !self.cpus.is_empty() {
            self.set_cpu_affinity();
        }
        
        let mut shutdown_rx = self.shutdown_rx;
        
        loop {
            tokio::select! {
                // Handle shutdown signal
                _ = &mut shutdown_rx => {
                    info!("I/O worker {} received shutdown signal", self.name);
                    break;
                }
                
                // Process I/O requests
                _ = self.process_requests() => {
                    // Continue processing
                }
                
                // Process completions periodically
                _ = tokio::time::sleep(Duration::from_micros(self.config.polling_interval_us)) => {
                    self.process_all_completions().await;
                }
            }
        }
        
        info!("I/O worker {} shutdown complete", self.name);
    }
    
    fn set_cpu_affinity(&self) {
        // This would set CPU affinity for the current thread
        // Implementation depends on platform-specific APIs
        debug!("Setting CPU affinity for worker {} to CPUs: {:?}", 
               self.name, self.cpus);
    }
    
    async fn process_requests(&self) {
        let mut rx = self.request_rx.lock();
        
        // Process up to batch size requests
        for _ in 0..self.config.batch_completion_size {
            match rx.try_recv() {
                Ok(request) => {
                    if let Err(e) = self.handle_request(request).await {
                        error!("Failed to handle I/O request: {}", e);
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    info!("Request channel disconnected for worker {}", self.name);
                    return;
                }
            }
        }
    }
    
    async fn handle_request(&self, request: IoRequest) -> Result<()> {
        // Find the ring for this device
        if let Some(ring_ref) = self.rings.get(&request.device_id) {
            let mut ring = ring_ref.lock();
            ring.submit_request(request)?;
        } else {
            // Send error back to requester
            let _ = request.completion_tx.send(Err(anyhow!(
                "Device not found: {}", request.device_id
            )));
        }
        
        Ok(())
    }
    
    async fn process_all_completions(&self) {
        for entry in self.rings.iter() {
            let mut ring = entry.value().lock();
            
            match ring.process_completions() {
                Ok(completed) => {
                    if completed > 0 {
                        self.stats.completed_requests.fetch_add(completed as u64, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    error!("Failed to process completions for device {}: {}", 
                           entry.key(), e);
                }
            }
        }
    }
}

/// Detect hardware acceleration capabilities
pub fn detect_hardware_capabilities() -> HardwareCapabilities {
    let mut capabilities = HardwareCapabilities::default();
    
    // Detect CPU features
    if is_x86_feature_detected!("aes") {
        capabilities.aes_ni = true;
    }
    
    if is_x86_feature_detected!("avx2") {
        capabilities.avx2 = true;
    }
    
    if is_x86_feature_detected!("sha") {
        capabilities.sha_extensions = true;
    }
    
    // Detect page sizes
    capabilities.huge_pages_2mb = check_huge_page_support(2 * 1024 * 1024);
    capabilities.huge_pages_1gb = check_huge_page_support(1024 * 1024 * 1024);
    
    // Detect io_uring features
    if let Ok(ring) = IoUring::new(16) {
        let probe = Probe::new();
        capabilities.io_uring_available = true;
        
        // Check for specific io_uring features
        capabilities.io_uring_sqpoll = probe.is_supported(opcode::Read::CODE);
        capabilities.io_uring_fast_poll = true; // Assume available in recent kernels
    }
    
    info!("Detected hardware capabilities: {:?}", capabilities);
    capabilities
}

/// Hardware acceleration capabilities
#[derive(Debug, Clone, Default)]
pub struct HardwareCapabilities {
    pub aes_ni: bool,
    pub avx2: bool,
    pub sha_extensions: bool,
    pub huge_pages_2mb: bool,
    pub huge_pages_1gb: bool,
    pub io_uring_available: bool,
    pub io_uring_sqpoll: bool,
    pub io_uring_fast_poll: bool,
}

/// Check if huge page support is available
fn check_huge_page_support(page_size: usize) -> bool {
    // Try to allocate a small huge page mapping
    let ptr = unsafe {
        mmap(
            None,
            page_size.try_into().unwrap(),
            ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
            MapFlags::MAP_PRIVATE | MapFlags::MAP_ANONYMOUS | MapFlags::MAP_HUGETLB,
            -1,
            0,
        )
    };
    
    match ptr {
        Ok(addr) => {
            // Huge page allocation succeeded
            unsafe {
                let _ = munmap(addr, page_size.try_into().unwrap());
            }
            true
        }
        Err(_) => false,
    }
}