use anyhow::{Result, anyhow, Context};
use blake3::Hasher as Blake3Hasher;
use crc32fast::Hasher as Crc32Hasher;
use dashmap::DashMap;
use parking_lot::{RwLock, Mutex};
use rand::{Rng, thread_rng};
use rocksdb::{DB, ColumnFamily, Options, WriteOptions, ReadOptions, WriteBatch};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicU64, AtomicUsize, Ordering}};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot, RwLock as AsyncRwLock, Semaphore};
use tracing::{debug, info, warn, error, trace};
use uuid::Uuid;

use galleon_common::types::{DeviceId, ChunkId, VolumeId, NodeId};
use galleon_common::error::StorageError;
use crate::device_manager::ClaimedDevice;
use crate::io_engine::{IoEngine, IoRequest, IoResponse, IoOpType, IoPriority, IoBuffer};

/// Chunk size configurations (64MB to 1GB)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkSize {
    Small = 64 * 1024 * 1024,      // 64MB
    Medium = 128 * 1024 * 1024,    // 128MB
    Large = 256 * 1024 * 1024,     // 256MB
    XLarge = 512 * 1024 * 1024,    // 512MB
    XXLarge = 1024 * 1024 * 1024,  // 1GB
}

impl ChunkSize {
    pub fn as_bytes(&self) -> u64 {
        *self as u64
    }
    
    pub fn from_bytes(bytes: u64) -> Self {
        match bytes {
            x if x <= 64 * 1024 * 1024 => ChunkSize::Small,
            x if x <= 128 * 1024 * 1024 => ChunkSize::Medium,
            x if x <= 256 * 1024 * 1024 => ChunkSize::Large,
            x if x <= 512 * 1024 * 1024 => ChunkSize::XLarge,
            _ => ChunkSize::XXLarge,
        }
    }
}

/// Chunk metadata stored in the metadata store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub chunk_id: ChunkId,
    pub volume_id: VolumeId,
    pub chunk_index: u64,
    pub size: u64,
    pub device_id: DeviceId,
    pub device_offset: u64,
    pub checksum: ChunkChecksum,
    pub compression: Option<CompressionType>,
    pub encryption: Option<EncryptionMetadata>,
    pub created_at: SystemTime,
    pub last_accessed: SystemTime,
    pub access_count: u64,
    pub reference_count: u32,
    pub status: ChunkStatus,
}

/// Chunk checksum for integrity verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChecksum {
    pub algorithm: ChecksumAlgorithm,
    pub value: Vec<u8>,
}

/// Supported checksum algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChecksumAlgorithm {
    Crc32c,    // Hardware-accelerated CRC32C
    Blake3,    // BLAKE3 hash
    Sha256,    // SHA-256 (for compatibility)
}

/// Compression types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionType {
    None,
    Lz4,
    Zstd,
    Snappy,
}

/// Encryption metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionMetadata {
    pub algorithm: EncryptionAlgorithm,
    pub key_id: String,
    pub iv: Vec<u8>,
    pub auth_tag: Option<Vec<u8>>,
}

/// Encryption algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionAlgorithm {
    None,
    Aes256Gcm,
    ChaCha20Poly1305,
}

/// Chunk status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkStatus {
    Writing,     // Currently being written
    Available,   // Ready for read/write
    Replicating, // Being replicated
    Corrupted,   // Checksum mismatch detected
    Deleted,     // Marked for deletion
}

/// Volume topology information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeTopology {
    pub volume_id: VolumeId,
    pub total_size: u64,
    pub chunk_size: ChunkSize,
    pub replication_factor: u32,
    pub erasure_coding: Option<ErasureCodingConfig>,
    pub placement_policy: PlacementPolicy,
    pub chunks: BTreeMap<u64, ChunkId>, // chunk_index -> chunk_id
    pub device_distribution: HashMap<DeviceId, Vec<ChunkId>>,
    pub created_at: SystemTime,
    pub last_modified: SystemTime,
}

/// Erasure coding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErasureCodingConfig {
    pub data_blocks: u32,
    pub parity_blocks: u32,
    pub algorithm: ErasureCodingAlgorithm,
}

/// Erasure coding algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErasureCodingAlgorithm {
    ReedSolomon,
    ReedSolomonSimd,
}

/// Chunk placement policies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlacementPolicy {
    /// Distribute chunks across devices for maximum performance
    MaximumDistribution {
        prefer_local_node: bool,
        max_chunks_per_device: u64,
        max_chunks_per_node: u64,
    },
    /// Keep chunks local for minimum latency
    LocalityPreferred {
        numa_node_preference: Option<u32>,
        device_type_preference: Option<crate::device_manager::DeviceType>,
    },
    /// Custom placement based on performance characteristics
    PerformanceBased {
        min_iops_per_device: u64,
        min_bandwidth_per_device: u64,
        max_latency_us: u64,
    },
}

/// Write-ahead log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    pub entry_id: Uuid,
    pub operation: WalOperation,
    pub timestamp: SystemTime,
    pub device_id: DeviceId,
    pub committed: bool,
}

/// WAL operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalOperation {
    WriteChunk {
        chunk_id: ChunkId,
        offset: u64,
        size: u64,
        checksum: ChunkChecksum,
    },
    DeleteChunk {
        chunk_id: ChunkId,
    },
    UpdateMetadata {
        chunk_id: ChunkId,
        metadata: ChunkMetadata,
    },
}

/// Chunk manager for handling millions of chunks
pub struct ChunkManager {
    /// Metadata database
    metadata_db: Arc<DB>,
    
    /// Volume topology cache
    volume_topologies: Arc<DashMap<VolumeId, Arc<VolumeTopology>>>,
    
    /// Chunk metadata cache
    chunk_cache: Arc<DashMap<ChunkId, Arc<ChunkMetadata>>>,
    
    /// Device chunk mapping
    device_chunks: Arc<DashMap<DeviceId, Arc<RwLock<Vec<ChunkId>>>>>,
    
    /// I/O engine for chunk operations
    io_engine: Arc<IoEngine>,
    
    /// Write-ahead logs per device
    device_wals: Arc<DashMap<DeviceId, Arc<Mutex<WriteAheadLog>>>>,
    
    /// Configuration
    config: ChunkManagerConfig,
    
    /// Statistics
    stats: Arc<ChunkManagerStats>,
    
    /// Node ID
    node_id: NodeId,
    
    /// Background task handles
    background_tasks: Vec<tokio::task::JoinHandle<()>>,
}

/// Write-ahead log for device
pub struct WriteAheadLog {
    entries: VecDeque<WalEntry>,
    device_id: DeviceId,
    log_file: PathBuf,
    next_sequence: AtomicU64,
    uncommitted_count: AtomicUsize,
    last_sync: Instant,
}

/// Chunk manager configuration
#[derive(Debug, Clone)]
pub struct ChunkManagerConfig {
    pub metadata_db_path: PathBuf,
    pub wal_dir: PathBuf,
    pub cache_size_mb: usize,
    pub wal_batch_size: usize,
    pub wal_sync_interval: Duration,
    pub checksum_algorithm: ChecksumAlgorithm,
    pub enable_compression: bool,
    pub enable_encryption: bool,
    pub gc_interval: Duration,
    pub max_chunks_per_device: u64,
    pub enable_hardware_acceleration: bool,
}

impl Default for ChunkManagerConfig {
    fn default() -> Self {
        Self {
            metadata_db_path: PathBuf::from("/var/lib/galleonfs/metadata"),
            wal_dir: PathBuf::from("/var/lib/galleonfs/wal"),
            cache_size_mb: 1024, // 1GB cache
            wal_batch_size: 100,
            wal_sync_interval: Duration::from_millis(100),
            checksum_algorithm: ChecksumAlgorithm::Blake3,
            enable_compression: true,
            enable_encryption: true,
            gc_interval: Duration::from_secs(300), // 5 minutes
            max_chunks_per_device: 1_000_000, // 1M chunks per device
            enable_hardware_acceleration: true,
        }
    }
}

/// Chunk manager statistics
#[derive(Debug, Default)]
pub struct ChunkManagerStats {
    pub total_chunks: AtomicU64,
    pub total_bytes_stored: AtomicU64,
    pub chunks_written: AtomicU64,
    pub chunks_read: AtomicU64,
    pub chunks_deleted: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub checksum_verifications: AtomicU64,
    pub checksum_failures: AtomicU64,
    pub compression_ratio: AtomicU64, // Multiplied by 1000 for precision
    pub wal_entries_written: AtomicU64,
    pub wal_syncs_performed: AtomicU64,
}

impl ChunkManager {
    /// Create a new chunk manager
    pub async fn new(
        node_id: NodeId,
        io_engine: Arc<IoEngine>,
        config: ChunkManagerConfig,
    ) -> Result<Self> {
        // Create directories
        tokio::fs::create_dir_all(&config.metadata_db_path).await?;
        tokio::fs::create_dir_all(&config.wal_dir).await?;
        
        // Open metadata database
        let mut db_options = Options::default();
        db_options.create_if_missing(true);
        db_options.set_write_buffer_size(config.cache_size_mb * 1024 * 1024 / 8);
        db_options.set_max_write_buffer_number(8);
        db_options.set_target_file_size_base(64 * 1024 * 1024); // 64MB
        db_options.set_level_zero_file_num_compaction_trigger(4);
        db_options.set_level_zero_slowdown_writes_trigger(20);
        db_options.set_level_zero_stop_writes_trigger(36);
        db_options.set_max_background_jobs(4);
        db_options.set_max_bytes_for_level_base(512 * 1024 * 1024); // 512MB
        
        let metadata_db = Arc::new(DB::open(&db_options, &config.metadata_db_path)?);
        
        let manager = Self {
            metadata_db,
            volume_topologies: Arc::new(DashMap::new()),
            chunk_cache: Arc::new(DashMap::new()),
            device_chunks: Arc::new(DashMap::new()),
            io_engine,
            device_wals: Arc::new(DashMap::new()),
            config,
            stats: Arc::new(ChunkManagerStats::default()),
            node_id,
            background_tasks: Vec::new(),
        };
        
        // Load existing volume topologies
        manager.load_volume_topologies().await?;
        
        info!("Chunk manager initialized with {} cached volumes", 
              manager.volume_topologies.len());
        
        Ok(manager)
    }
    
    /// Load volume topologies from metadata database
    async fn load_volume_topologies(&self) -> Result<()> {
        let cf_handle = self.metadata_db.cf_handle("volume_topologies");
        let iter = self.metadata_db.iterator(cf_handle.unwrap_or_else(|| self.metadata_db.cf_handle("default").unwrap()));
        
        for item in iter {
            let (key, value) = item?;
            if let Ok(volume_id) = String::from_utf8(key.to_vec()) {
                if let Ok(topology) = bincode::deserialize::<VolumeTopology>(&value) {
                    let volume_id = VolumeId::from(volume_id);
                    self.volume_topologies.insert(volume_id, Arc::new(topology));
                }
            }
        }
        
        Ok(())
    }
    
    /// Create a new volume with specified topology
    pub async fn create_volume(
        &self,
        volume_id: VolumeId,
        total_size: u64,
        chunk_size: ChunkSize,
        placement_policy: PlacementPolicy,
        devices: &[DeviceId],
    ) -> Result<()> {
        info!("Creating volume {} with size {} bytes, chunk size {} bytes", 
              volume_id, total_size, chunk_size.as_bytes());
        
        let chunks_needed = (total_size + chunk_size.as_bytes() - 1) / chunk_size.as_bytes();
        
        if chunks_needed > self.config.max_chunks_per_device * devices.len() as u64 {
            return Err(anyhow!(
                "Volume size too large: needs {} chunks, but only {} available",
                chunks_needed,
                self.config.max_chunks_per_device * devices.len() as u64
            ));
        }
        
        let mut chunks = BTreeMap::new();
        let mut device_distribution = HashMap::new();
        
        // Initialize device distribution
        for device_id in devices {
            device_distribution.insert(device_id.clone(), Vec::new());
        }
        
        // Create chunk allocation plan
        for chunk_index in 0..chunks_needed {
            let chunk_id = ChunkId::new(&format!("{}:chunk:{}", volume_id, chunk_index));
            chunks.insert(chunk_index, chunk_id.clone());
            
            // Select device based on placement policy
            let device_id = self.select_device_for_chunk(
                &placement_policy,
                devices,
                &device_distribution,
                chunk_index,
            ).await?;
            
            device_distribution.get_mut(&device_id)
                .unwrap()
                .push(chunk_id);
        }
        
        let topology = VolumeTopology {
            volume_id: volume_id.clone(),
            total_size,
            chunk_size,
            replication_factor: 1, // TODO: Make configurable
            erasure_coding: None,  // TODO: Add erasure coding support
            placement_policy,
            chunks,
            device_distribution,
            created_at: SystemTime::now(),
            last_modified: SystemTime::now(),
        };
        
        // Store topology in database
        self.store_volume_topology(&topology).await?;
        
        // Cache topology
        self.volume_topologies.insert(volume_id, Arc::new(topology));
        
        info!("Successfully created volume {} with {} chunks across {} devices", 
              volume_id, chunks_needed, devices.len());
        
        Ok(())
    }
    
    /// Select device for chunk based on placement policy
    async fn select_device_for_chunk(
        &self,
        policy: &PlacementPolicy,
        devices: &[DeviceId],
        current_distribution: &HashMap<DeviceId, Vec<ChunkId>>,
        chunk_index: u64,
    ) -> Result<DeviceId> {
        match policy {
            PlacementPolicy::MaximumDistribution { max_chunks_per_device, .. } => {
                // Find device with least chunks
                let mut min_chunks = u64::MAX;
                let mut selected_device = devices[0].clone();
                
                for device_id in devices {
                    let chunk_count = current_distribution.get(device_id).unwrap().len() as u64;
                    if chunk_count < min_chunks && chunk_count < *max_chunks_per_device {
                        min_chunks = chunk_count;
                        selected_device = device_id.clone();
                    }
                }
                
                Ok(selected_device)
            }
            PlacementPolicy::LocalityPreferred { .. } => {
                // Simple round-robin for now
                Ok(devices[chunk_index as usize % devices.len()].clone())
            }
            PlacementPolicy::PerformanceBased { .. } => {
                // Select based on device performance characteristics
                // For now, use simple round-robin
                Ok(devices[chunk_index as usize % devices.len()].clone())
            }
        }
    }
    
    /// Store volume topology in database
    async fn store_volume_topology(&self, topology: &VolumeTopology) -> Result<()> {
        let key = topology.volume_id.to_string();
        let value = bincode::serialize(topology)?;
        
        let cf_handle = self.metadata_db.cf_handle("volume_topologies")
            .unwrap_or_else(|| self.metadata_db.cf_handle("default").unwrap());
        
        self.metadata_db.put_cf(cf_handle, key.as_bytes(), &value)?;
        
        Ok(())
    }
    
    /// Write chunk data to storage
    pub async fn write_chunk(
        &self,
        volume_id: &VolumeId,
        chunk_index: u64,
        data: &[u8],
    ) -> Result<ChunkId> {
        let topology = self.volume_topologies.get(volume_id)
            .ok_or_else(|| anyhow!("Volume not found: {}", volume_id))?;
        
        let chunk_id = topology.chunks.get(&chunk_index)
            .ok_or_else(|| anyhow!("Chunk index {} not found in volume {}", chunk_index, volume_id))?;
        
        let device_id = self.find_device_for_chunk(chunk_id, &topology)?;
        
        // Calculate checksum
        let checksum = self.calculate_checksum(data)?;
        
        // Allocate device offset
        let device_offset = self.allocate_device_offset(&device_id, data.len() as u64).await?;
        
        // Create chunk metadata
        let metadata = ChunkMetadata {
            chunk_id: chunk_id.clone(),
            volume_id: volume_id.clone(),
            chunk_index,
            size: data.len() as u64,
            device_id: device_id.clone(),
            device_offset,
            checksum,
            compression: None, // TODO: Add compression
            encryption: None,  // TODO: Add encryption
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
            reference_count: 1,
            status: ChunkStatus::Writing,
        };
        
        // Write to WAL first
        self.write_wal_entry(&device_id, WalOperation::WriteChunk {
            chunk_id: chunk_id.clone(),
            offset: device_offset,
            size: data.len() as u64,
            checksum: metadata.checksum.clone(),
        }).await?;
        
        // Create I/O buffer
        let mut io_buffer = self.io_engine.create_buffer(data.len(), None)?;
        io_buffer.as_mut_slice().copy_from_slice(data);
        
        // Submit I/O request
        let request = IoRequest {
            request_id: Uuid::new_v4(),
            device_id: device_id.clone(),
            op_type: IoOpType::Write,
            offset: device_offset,
            data: io_buffer,
            completion_tx: oneshot::channel().0, // Will be replaced
            priority: IoPriority::Normal,
            numa_node: None,
            submitted_at: Instant::now(),
        };
        
        let response = self.io_engine.submit_io(request).await?;
        
        if response.bytes_transferred != data.len() {
            return Err(anyhow!(
                "Incomplete write: expected {} bytes, wrote {} bytes",
                data.len(),
                response.bytes_transferred
            ));
        }
        
        // Update chunk status
        let mut final_metadata = metadata;
        final_metadata.status = ChunkStatus::Available;
        
        // Store metadata
        self.store_chunk_metadata(&final_metadata).await?;
        
        // Cache metadata
        self.chunk_cache.insert(chunk_id.clone(), Arc::new(final_metadata));
        
        // Update statistics
        self.stats.chunks_written.fetch_add(1, Ordering::Relaxed);
        self.stats.total_bytes_stored.fetch_add(data.len() as u64, Ordering::Relaxed);
        self.stats.total_chunks.fetch_add(1, Ordering::Relaxed);
        
        info!("Successfully wrote chunk {} ({} bytes) to device {} at offset {}", 
              chunk_id, data.len(), device_id, device_offset);
        
        Ok(chunk_id.clone())
    }
    
    /// Read chunk data from storage
    pub async fn read_chunk(
        &self,
        chunk_id: &ChunkId,
    ) -> Result<Vec<u8>> {
        // Try cache first
        let metadata = if let Some(cached) = self.chunk_cache.get(chunk_id) {
            self.stats.cache_hits.fetch_add(1, Ordering::Relaxed);
            cached.clone()
        } else {
            self.stats.cache_misses.fetch_add(1, Ordering::Relaxed);
            let metadata = self.load_chunk_metadata(chunk_id).await?;
            self.chunk_cache.insert(chunk_id.clone(), metadata.clone());
            metadata
        };
        
        if metadata.status != ChunkStatus::Available {
            return Err(anyhow!("Chunk {} is not available (status: {:?})", 
                              chunk_id, metadata.status));
        }
        
        // Create I/O buffer
        let mut io_buffer = self.io_engine.create_buffer(metadata.size as usize, None)?;
        
        // Submit read request
        let request = IoRequest {
            request_id: Uuid::new_v4(),
            device_id: metadata.device_id.clone(),
            op_type: IoOpType::Read,
            offset: metadata.device_offset,
            data: io_buffer,
            completion_tx: oneshot::channel().0, // Will be replaced
            priority: IoPriority::Normal,
            numa_node: None,
            submitted_at: Instant::now(),
        };
        
        let response = self.io_engine.submit_io(request).await?;
        
        if response.bytes_transferred != metadata.size as usize {
            return Err(anyhow!(
                "Incomplete read: expected {} bytes, read {} bytes",
                metadata.size,
                response.bytes_transferred
            ));
        }
        
        // Get data from buffer
        let data = response.request_id; // This would actually contain the data
        // TODO: Fix this - we need to get the actual data from the I/O response
        let data = vec![0u8; metadata.size as usize]; // Placeholder
        
        // Verify checksum
        let calculated_checksum = self.calculate_checksum(&data)?;
        if calculated_checksum.value != metadata.checksum.value {
            self.stats.checksum_failures.fetch_add(1, Ordering::Relaxed);
            
            // Mark chunk as corrupted
            let mut corrupted_metadata = metadata.as_ref().clone();
            corrupted_metadata.status = ChunkStatus::Corrupted;
            self.store_chunk_metadata(&corrupted_metadata).await?;
            
            return Err(anyhow!("Checksum verification failed for chunk {}", chunk_id));
        }
        
        self.stats.checksum_verifications.fetch_add(1, Ordering::Relaxed);
        
        // Update access statistics
        let mut updated_metadata = metadata.as_ref().clone();
        updated_metadata.last_accessed = SystemTime::now();
        updated_metadata.access_count += 1;
        self.store_chunk_metadata(&updated_metadata).await?;
        
        self.stats.chunks_read.fetch_add(1, Ordering::Relaxed);
        
        Ok(data)
    }
    
    /// Delete chunk from storage
    pub async fn delete_chunk(&self, chunk_id: &ChunkId) -> Result<()> {
        let metadata = self.load_chunk_metadata(chunk_id).await?;
        
        // Write to WAL
        self.write_wal_entry(&metadata.device_id, WalOperation::DeleteChunk {
            chunk_id: chunk_id.clone(),
        }).await?;
        
        // Submit discard request for the chunk region
        let mut io_buffer = self.io_engine.create_buffer(metadata.size as usize, None)?;
        
        let request = IoRequest {
            request_id: Uuid::new_v4(),
            device_id: metadata.device_id.clone(),
            op_type: IoOpType::Discard,
            offset: metadata.device_offset,
            data: io_buffer,
            completion_tx: oneshot::channel().0,
            priority: IoPriority::Low,
            numa_node: None,
            submitted_at: Instant::now(),
        };
        
        let _response = self.io_engine.submit_io(request).await?;
        
        // Mark chunk as deleted
        let mut deleted_metadata = metadata;
        deleted_metadata.status = ChunkStatus::Deleted;
        self.store_chunk_metadata(&deleted_metadata).await?;
        
        // Remove from cache
        self.chunk_cache.remove(chunk_id);
        
        self.stats.chunks_deleted.fetch_add(1, Ordering::Relaxed);
        self.stats.total_chunks.fetch_sub(1, Ordering::Relaxed);
        self.stats.total_bytes_stored.fetch_sub(deleted_metadata.size, Ordering::Relaxed);
        
        info!("Successfully deleted chunk {}", chunk_id);
        Ok(())
    }
    
    /// Find device that contains the chunk
    fn find_device_for_chunk(
        &self,
        chunk_id: &ChunkId,
        topology: &VolumeTopology,
    ) -> Result<DeviceId> {
        for (device_id, chunks) in &topology.device_distribution {
            if chunks.contains(chunk_id) {
                return Ok(device_id.clone());
            }
        }
        
        Err(anyhow!("Chunk {} not found in any device", chunk_id))
    }
    
    /// Calculate checksum for data
    fn calculate_checksum(&self, data: &[u8]) -> Result<ChunkChecksum> {
        let value = match self.config.checksum_algorithm {
            ChecksumAlgorithm::Crc32c => {
                let mut hasher = Crc32Hasher::new();
                hasher.update(data);
                hasher.finalize().to_le_bytes().to_vec()
            }
            ChecksumAlgorithm::Blake3 => {
                let mut hasher = Blake3Hasher::new();
                hasher.update(data);
                hasher.finalize().as_bytes().to_vec()
            }
            ChecksumAlgorithm::Sha256 => {
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(data);
                hasher.finalize().to_vec()
            }
        };
        
        Ok(ChunkChecksum {
            algorithm: self.config.checksum_algorithm,
            value,
        })
    }
    
    /// Allocate offset on device for chunk
    async fn allocate_device_offset(&self, device_id: &DeviceId, size: u64) -> Result<u64> {
        // Simple allocation for now - this would be more sophisticated in production
        // with free space tracking and fragmentation management
        static NEXT_OFFSET: AtomicU64 = AtomicU64::new(0);
        let offset = NEXT_OFFSET.fetch_add(size, Ordering::Relaxed);
        Ok(offset)
    }
    
    /// Write entry to device WAL
    async fn write_wal_entry(&self, device_id: &DeviceId, operation: WalOperation) -> Result<()> {
        let wal = self.get_or_create_wal(device_id).await?;
        let mut wal_guard = wal.lock();
        
        let entry = WalEntry {
            entry_id: Uuid::new_v4(),
            operation,
            timestamp: SystemTime::now(),
            device_id: device_id.clone(),
            committed: false,
        };
        
        wal_guard.entries.push_back(entry);
        wal_guard.uncommitted_count.fetch_add(1, Ordering::Relaxed);
        
        self.stats.wal_entries_written.fetch_add(1, Ordering::Relaxed);
        
        // Sync WAL if needed
        if wal_guard.uncommitted_count.load(Ordering::Relaxed) >= self.config.wal_batch_size ||
           wal_guard.last_sync.elapsed() >= self.config.wal_sync_interval {
            self.sync_wal(&mut wal_guard).await?;
        }
        
        Ok(())
    }
    
    /// Get or create WAL for device
    async fn get_or_create_wal(&self, device_id: &DeviceId) -> Result<Arc<Mutex<WriteAheadLog>>> {
        if let Some(wal) = self.device_wals.get(device_id) {
            return Ok(wal.clone());
        }
        
        let log_file = self.config.wal_dir.join(format!("{}.wal", device_id));
        let wal = Arc::new(Mutex::new(WriteAheadLog {
            entries: VecDeque::new(),
            device_id: device_id.clone(),
            log_file,
            next_sequence: AtomicU64::new(1),
            uncommitted_count: AtomicUsize::new(0),
            last_sync: Instant::now(),
        }));
        
        self.device_wals.insert(device_id.clone(), wal.clone());
        Ok(wal)
    }
    
    /// Sync WAL to disk
    async fn sync_wal(&self, wal: &mut WriteAheadLog) -> Result<()> {
        // Write uncommitted entries to disk
        // This is a simplified implementation
        wal.uncommitted_count.store(0, Ordering::Relaxed);
        wal.last_sync = Instant::now();
        
        self.stats.wal_syncs_performed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    
    /// Store chunk metadata in database
    async fn store_chunk_metadata(&self, metadata: &ChunkMetadata) -> Result<()> {
        let key = metadata.chunk_id.to_string();
        let value = bincode::serialize(metadata)?;
        
        let cf_handle = self.metadata_db.cf_handle("chunk_metadata")
            .unwrap_or_else(|| self.metadata_db.cf_handle("default").unwrap());
        
        self.metadata_db.put_cf(cf_handle, key.as_bytes(), &value)?;
        Ok(())
    }
    
    /// Load chunk metadata from database
    async fn load_chunk_metadata(&self, chunk_id: &ChunkId) -> Result<Arc<ChunkMetadata>> {
        let key = chunk_id.to_string();
        
        let cf_handle = self.metadata_db.cf_handle("chunk_metadata")
            .unwrap_or_else(|| self.metadata_db.cf_handle("default").unwrap());
        
        if let Some(value) = self.metadata_db.get_cf(cf_handle, key.as_bytes())? {
            let metadata: ChunkMetadata = bincode::deserialize(&value)?;
            Ok(Arc::new(metadata))
        } else {
            Err(anyhow!("Chunk metadata not found: {}", chunk_id))
        }
    }
    
    /// Get volume topology
    pub fn get_volume_topology(&self, volume_id: &VolumeId) -> Option<Arc<VolumeTopology>> {
        self.volume_topologies.get(volume_id).map(|entry| entry.clone())
    }
    
    /// List chunks for a volume
    pub async fn list_volume_chunks(&self, volume_id: &VolumeId) -> Result<Vec<ChunkId>> {
        if let Some(topology) = self.volume_topologies.get(volume_id) {
            Ok(topology.chunks.values().cloned().collect())
        } else {
            Err(anyhow!("Volume not found: {}", volume_id))
        }
    }
    
    /// Get chunk manager statistics
    pub fn get_stats(&self) -> ChunkManagerStats {
        ChunkManagerStats {
            total_chunks: AtomicU64::new(self.stats.total_chunks.load(Ordering::Relaxed)),
            total_bytes_stored: AtomicU64::new(self.stats.total_bytes_stored.load(Ordering::Relaxed)),
            chunks_written: AtomicU64::new(self.stats.chunks_written.load(Ordering::Relaxed)),
            chunks_read: AtomicU64::new(self.stats.chunks_read.load(Ordering::Relaxed)),
            chunks_deleted: AtomicU64::new(self.stats.chunks_deleted.load(Ordering::Relaxed)),
            cache_hits: AtomicU64::new(self.stats.cache_hits.load(Ordering::Relaxed)),
            cache_misses: AtomicU64::new(self.stats.cache_misses.load(Ordering::Relaxed)),
            checksum_verifications: AtomicU64::new(self.stats.checksum_verifications.load(Ordering::Relaxed)),
            checksum_failures: AtomicU64::new(self.stats.checksum_failures.load(Ordering::Relaxed)),
            compression_ratio: AtomicU64::new(self.stats.compression_ratio.load(Ordering::Relaxed)),
            wal_entries_written: AtomicU64::new(self.stats.wal_entries_written.load(Ordering::Relaxed)),
            wal_syncs_performed: AtomicU64::new(self.stats.wal_syncs_performed.load(Ordering::Relaxed)),
        }
    }
    
    /// Start background tasks
    pub fn start_background_tasks(&mut self) -> Result<()> {
        // WAL sync task
        let device_wals = self.device_wals.clone();
        let config = self.config.clone();
        let stats = self.stats.clone();
        
        let wal_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.wal_sync_interval);
            
            loop {
                interval.tick().await;
                
                for entry in device_wals.iter() {
                    let mut wal = entry.value().lock();
                    if wal.uncommitted_count.load(Ordering::Relaxed) > 0 {
                        // Sync WAL implementation would go here
                        wal.uncommitted_count.store(0, Ordering::Relaxed);
                        wal.last_sync = Instant::now();
                        stats.wal_syncs_performed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
        
        self.background_tasks.push(wal_task);
        
        // Garbage collection task
        let chunk_cache = self.chunk_cache.clone();
        let gc_interval = self.config.gc_interval;
        
        let gc_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(gc_interval);
            
            loop {
                interval.tick().await;
                
                // Remove old entries from cache
                let now = SystemTime::now();
                let mut to_remove = Vec::new();
                
                for entry in chunk_cache.iter() {
                    let metadata = entry.value();
                    if let Ok(elapsed) = now.duration_since(metadata.last_accessed) {
                        if elapsed > Duration::from_secs(3600) && metadata.access_count < 10 {
                            to_remove.push(entry.key().clone());
                        }
                    }
                }
                
                for chunk_id in to_remove {
                    chunk_cache.remove(&chunk_id);
                }
            }
        });
        
        self.background_tasks.push(gc_task);
        
        info!("Started {} background tasks for chunk manager", self.background_tasks.len());
        Ok(())
    }
    
    /// Shutdown chunk manager
    pub async fn shutdown(self) -> Result<()> {
        info!("Shutting down chunk manager");
        
        // Cancel background tasks
        for task in self.background_tasks {
            task.abort();
        }
        
        // Sync all WALs
        for entry in self.device_wals.iter() {
            let mut wal = entry.value().lock();
            if wal.uncommitted_count.load(Ordering::Relaxed) > 0 {
                // Final WAL sync
                wal.uncommitted_count.store(0, Ordering::Relaxed);
            }
        }
        
        info!("Chunk manager shutdown complete");
        Ok(())
    }
}