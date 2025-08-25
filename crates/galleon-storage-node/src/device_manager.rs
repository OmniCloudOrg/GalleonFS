use anyhow::{Result, anyhow, Context};
use dashmap::DashMap;
use nix::fcntl::{FlockArg, flock};
use nix::sys::stat;
use parking_lot::RwLock;
use procfs::sys::block::BlockDevice;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{System, SystemExt};
use tokio::sync::RwLock as AsyncRwLock;
use tokio::time::sleep;
use tracing::{debug, info, warn, error};
use uuid::Uuid;

use galleon_common::types::{DeviceId, NodeId, ChunkId};
use galleon_common::error::StorageError;

/// Represents a storage device that has been claimed exclusively
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedDevice {
    pub device_id: DeviceId,
    pub device_path: PathBuf,
    pub device_name: String,
    pub total_capacity: u64,
    pub available_capacity: u64,
    pub block_size: u32,
    pub numa_node: Option<u32>,
    pub device_type: DeviceType,
    pub smart_data: SmartData,
    pub claimed_at: chrono::DateTime<chrono::Utc>,
    pub lock_file: PathBuf,
    pub performance_class: PerformanceClass,
}

/// Device type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceType {
    NVMe,
    SSD,
    HDD,
    Optane,
    Memory,
    Unknown,
}

/// Performance classification for devices
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformanceClass {
    UltraHigh,  // NVMe, Optane
    High,       // High-end SSDs
    Medium,     // Standard SSDs
    Low,        // HDDs
}

/// SMART data for predictive failure detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartData {
    pub temperature: Option<u32>,
    pub power_on_hours: Option<u64>,
    pub power_cycle_count: Option<u64>,
    pub reallocated_sectors: Option<u32>,
    pub pending_sectors: Option<u32>,
    pub offline_uncorrectable: Option<u32>,
    pub wear_leveling_count: Option<u32>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    pub health_status: HealthStatus,
}

/// Device health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Excellent,
    Good,
    Warning,
    Critical,
    Failed,
}

/// NUMA topology information
#[derive(Debug, Clone)]
pub struct NumaTopology {
    pub nodes: HashMap<u32, NumaNode>,
    pub total_nodes: u32,
}

/// NUMA node information
#[derive(Debug, Clone)]
pub struct NumaNode {
    pub node_id: u32,
    pub cpus: Vec<u32>,
    pub memory_gb: u64,
    pub devices: Vec<DeviceId>,
}

/// Performance specification for device selection
#[derive(Debug, Clone)]
pub struct PerformanceSpec {
    pub min_iops: u64,
    pub min_bandwidth_mb: u64,
    pub max_latency_us: u64,
    pub preferred_device_types: Vec<DeviceType>,
    pub numa_locality: bool,
}

/// Multi-device manager for handling thousands of devices per node
pub struct MultiDeviceManager {
    /// Map of claimed devices
    claimed_devices: Arc<DashMap<DeviceId, ClaimedDevice>>,
    
    /// Device health monitoring data
    device_health: Arc<DashMap<DeviceId, HealthMonitor>>,
    
    /// NUMA topology information
    numa_topology: Arc<RwLock<NumaTopology>>,
    
    /// Mapping from device to NUMA node
    device_to_numa: Arc<DashMap<DeviceId, u32>>,
    
    /// Available devices cache
    available_devices: Arc<AsyncRwLock<HashMap<DeviceId, DeviceInfo>>>,
    
    /// System information
    system: Arc<parking_lot::Mutex<System>>,
    
    /// Configuration
    config: DeviceManagerConfig,
    
    /// Node ID
    node_id: NodeId,
    
    /// Device discovery running flag
    discovery_running: Arc<parking_lot::Mutex<bool>>,
}

/// Device information for discovery
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub device_id: DeviceId,
    pub path: PathBuf,
    pub name: String,
    pub capacity: u64,
    pub block_size: u32,
    pub device_type: DeviceType,
    pub numa_node: Option<u32>,
    pub is_claimed: bool,
}

/// Health monitoring for devices
#[derive(Debug)]
pub struct HealthMonitor {
    pub device_id: DeviceId,
    pub last_check: Instant,
    pub check_interval: Duration,
    pub smart_data: SmartData,
    pub performance_metrics: PerformanceMetrics,
}

/// Performance metrics for device monitoring
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub read_iops: f64,
    pub write_iops: f64,
    pub read_bandwidth: f64,
    pub write_bandwidth: f64,
    pub avg_latency_us: f64,
    pub queue_depth: u32,
    pub utilization_percent: f64,
}

/// Device manager configuration
#[derive(Debug, Clone)]
pub struct DeviceManagerConfig {
    pub max_devices_per_node: usize,
    pub device_discovery_interval: Duration,
    pub health_check_interval: Duration,
    pub smart_check_interval: Duration,
    pub enable_numa_optimization: bool,
    pub enable_hardware_accel: bool,
    pub lock_file_dir: PathBuf,
    pub allowed_device_patterns: Vec<String>,
    pub excluded_device_patterns: Vec<String>,
}

impl Default for DeviceManagerConfig {
    fn default() -> Self {
        Self {
            max_devices_per_node: 10000,
            device_discovery_interval: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(60),
            smart_check_interval: Duration::from_secs(300),
            enable_numa_optimization: true,
            enable_hardware_accel: true,
            lock_file_dir: PathBuf::from("/var/lib/galleonfs/locks"),
            allowed_device_patterns: vec![
                "/dev/nvme*".to_string(),
                "/dev/sd*".to_string(),
                "/dev/vd*".to_string(),
            ],
            excluded_device_patterns: vec![
                "/dev/sda".to_string(), // Boot disk
                "/dev/loop*".to_string(),
                "/dev/ram*".to_string(),
            ],
        }
    }
}

impl MultiDeviceManager {
    /// Create a new multi-device manager
    pub async fn new(node_id: NodeId, config: DeviceManagerConfig) -> Result<Self> {
        // Create lock file directory
        tokio::fs::create_dir_all(&config.lock_file_dir).await
            .context("Failed to create lock file directory")?;
        
        let manager = Self {
            claimed_devices: Arc::new(DashMap::new()),
            device_health: Arc::new(DashMap::new()),
            numa_topology: Arc::new(RwLock::new(NumaTopology {
                nodes: HashMap::new(),
                total_nodes: 0,
            })),
            device_to_numa: Arc::new(DashMap::new()),
            available_devices: Arc::new(AsyncRwLock::new(HashMap::new())),
            system: Arc::new(parking_lot::Mutex::new(System::new_all())),
            config,
            node_id,
            discovery_running: Arc::new(parking_lot::Mutex::new(false)),
        };
        
        // Initialize NUMA topology
        manager.detect_numa_topology().await?;
        
        // Start initial device discovery
        manager.discover_devices().await?;
        
        Ok(manager)
    }
    
    /// Detect NUMA topology for device optimization
    async fn detect_numa_topology(&self) -> Result<()> {
        info!("Detecting NUMA topology...");
        
        let mut topology = NumaTopology {
            nodes: HashMap::new(),
            total_nodes: 0,
        };
        
        // Read NUMA information from /sys/devices/system/node/
        let numa_path = Path::new("/sys/devices/system/node");
        if numa_path.exists() {
            let mut entries = tokio::fs::read_dir(numa_path).await?;
            while let Some(entry) = entries.next_entry().await? {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                
                if name_str.starts_with("node") {
                    if let Ok(node_id) = name_str[4..].parse::<u32>() {
                        let node_path = entry.path();
                        
                        // Read CPU list
                        let cpulist_path = node_path.join("cpulist");
                        let cpus = if cpulist_path.exists() {
                            let cpulist = tokio::fs::read_to_string(cpulist_path).await?;
                            self.parse_cpu_list(&cpulist.trim())?
                        } else {
                            Vec::new()
                        };
                        
                        // Read memory info
                        let meminfo_path = node_path.join("meminfo");
                        let memory_gb = if meminfo_path.exists() {
                            let meminfo = tokio::fs::read_to_string(meminfo_path).await?;
                            self.parse_memory_info(&meminfo)?
                        } else {
                            0
                        };
                        
                        topology.nodes.insert(node_id, NumaNode {
                            node_id,
                            cpus,
                            memory_gb,
                            devices: Vec::new(),
                        });
                        
                        topology.total_nodes = topology.total_nodes.max(node_id + 1);
                    }
                }
            }
        }
        
        if topology.nodes.is_empty() {
            // Fallback for non-NUMA systems
            topology.nodes.insert(0, NumaNode {
                node_id: 0,
                cpus: (0..num_cpus::get() as u32).collect(),
                memory_gb: 16, // Default
                devices: Vec::new(),
            });
            topology.total_nodes = 1;
        }
        
        info!("Detected {} NUMA nodes", topology.total_nodes);
        for (node_id, node) in &topology.nodes {
            debug!("NUMA node {}: {} CPUs, {} GB memory", 
                   node_id, node.cpus.len(), node.memory_gb);
        }
        
        *self.numa_topology.write() = topology;
        Ok(())
    }
    
    /// Parse CPU list from NUMA sysfs
    fn parse_cpu_list(&self, cpulist: &str) -> Result<Vec<u32>> {
        let mut cpus = Vec::new();
        
        for part in cpulist.split(',') {
            if part.contains('-') {
                let range: Vec<&str> = part.split('-').collect();
                if range.len() == 2 {
                    let start: u32 = range[0].parse()?;
                    let end: u32 = range[1].parse()?;
                    for cpu in start..=end {
                        cpus.push(cpu);
                    }
                }
            } else {
                cpus.push(part.parse()?);
            }
        }
        
        Ok(cpus)
    }
    
    /// Parse memory info from NUMA sysfs
    fn parse_memory_info(&self, meminfo: &str) -> Result<u64> {
        for line in meminfo.lines() {
            if line.starts_with("Node") && line.contains("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let kb: u64 = parts[2].parse()?;
                    return Ok(kb / 1024 / 1024); // Convert to GB
                }
            }
        }
        Ok(0)
    }
    
    /// Discover available storage devices
    pub async fn discover_devices(&self) -> Result<()> {
        {
            let mut running = self.discovery_running.lock();
            if *running {
                return Ok(());
            }
            *running = true;
        }
        
        info!("Starting device discovery...");
        let start_time = Instant::now();
        
        let mut discovered_devices = HashMap::new();
        
        // Scan /dev for block devices
        self.scan_dev_directory(&mut discovered_devices).await?;
        
        // Update NUMA device mapping
        self.update_numa_device_mapping(&discovered_devices).await?;
        
        // Update available devices cache
        {
            let mut available = self.available_devices.write().await;
            *available = discovered_devices;
        }
        
        let discovery_time = start_time.elapsed();
        let device_count = self.available_devices.read().await.len();
        
        info!("Device discovery completed in {:?}, found {} devices", 
              discovery_time, device_count);
        
        *self.discovery_running.lock() = false;
        Ok(())
    }
    
    /// Scan /dev directory for block devices
    async fn scan_dev_directory(&self, devices: &mut HashMap<DeviceId, DeviceInfo>) -> Result<()> {
        let dev_path = Path::new("/dev");
        let mut entries = tokio::fs::read_dir(dev_path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            
            // Check if device matches allowed patterns
            if !self.is_device_allowed(&path) {
                continue;
            }
            
            // Check if it's a block device
            if let Ok(metadata) = entry.metadata().await {
                if !metadata.file_type().is_block_device() {
                    continue;
                }
            }
            
            // Get device information
            match self.get_device_info(&path, &name).await {
                Ok(device_info) => {
                    devices.insert(device_info.device_id.clone(), device_info);
                }
                Err(e) => {
                    debug!("Failed to get info for device {}: {}", path.display(), e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Check if device is allowed based on patterns
    fn is_device_allowed(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        
        // Check excluded patterns first
        for pattern in &self.config.excluded_device_patterns {
            if self.matches_pattern(&path_str, pattern) {
                return false;
            }
        }
        
        // Check allowed patterns
        for pattern in &self.config.allowed_device_patterns {
            if self.matches_pattern(&path_str, pattern) {
                return true;
            }
        }
        
        false
    }
    
    /// Simple pattern matching with wildcards
    fn matches_pattern(&self, text: &str, pattern: &str) -> bool {
        if pattern.ends_with('*') {
            text.starts_with(&pattern[..pattern.len() - 1])
        } else {
            text == pattern
        }
    }
    
    /// Get detailed device information
    async fn get_device_info(&self, path: &Path, name: &str) -> Result<DeviceInfo> {
        // Get device capacity and block size
        let (capacity, block_size) = self.get_device_capacity(path).await?;
        
        // Determine device type
        let device_type = self.determine_device_type(name, path).await?;
        
        // Generate device ID
        let device_id = DeviceId::new(&format!("{}:{}", self.node_id, name));
        
        // Check if device is already claimed
        let is_claimed = self.claimed_devices.contains_key(&device_id);
        
        Ok(DeviceInfo {
            device_id,
            path: path.to_path_buf(),
            name: name.to_string(),
            capacity,
            block_size,
            device_type,
            numa_node: None, // Will be filled later
            is_claimed,
        })
    }
    
    /// Get device capacity and block size
    async fn get_device_capacity(&self, path: &Path) -> Result<(u64, u32)> {
        // Try to read from sysfs first
        let name = path.file_name()
            .ok_or_else(|| anyhow!("Invalid device path"))?
            .to_string_lossy();
        
        let sys_path = format!("/sys/block/{}/size", name);
        let logical_block_size_path = format!("/sys/block/{}/queue/logical_block_size", name);
        
        let capacity = if Path::new(&sys_path).exists() {
            let size_str = tokio::fs::read_to_string(&sys_path).await?;
            let sectors: u64 = size_str.trim().parse()?;
            sectors * 512 // Default sector size
        } else {
            // Fallback to using block device ioctls
            self.get_device_capacity_ioctl(path).await?
        };
        
        let block_size = if Path::new(&logical_block_size_path).exists() {
            let size_str = tokio::fs::read_to_string(&logical_block_size_path).await?;
            size_str.trim().parse().unwrap_or(4096)
        } else {
            4096 // Default block size
        };
        
        Ok((capacity, block_size))
    }
    
    /// Get device capacity using ioctls
    async fn get_device_capacity_ioctl(&self, path: &Path) -> Result<u64> {
        // This would use Linux ioctls to get device size
        // For now, we'll use a placeholder implementation
        warn!("Using placeholder capacity detection for {}", path.display());
        Ok(1024 * 1024 * 1024 * 1024) // 1TB placeholder
    }
    
    /// Determine device type based on name and characteristics
    async fn determine_device_type(&self, name: &str, path: &Path) -> Result<DeviceType> {
        if name.starts_with("nvme") {
            Ok(DeviceType::NVMe)
        } else if name.starts_with("sd") {
            // Check if it's an SSD or HDD by looking at rotational
            let rotational_path = format!("/sys/block/{}/queue/rotational", name);
            if Path::new(&rotational_path).exists() {
                let rotational_str = tokio::fs::read_to_string(&rotational_path).await?;
                if rotational_str.trim() == "0" {
                    Ok(DeviceType::SSD)
                } else {
                    Ok(DeviceType::HDD)
                }
            } else {
                Ok(DeviceType::Unknown)
            }
        } else if name.starts_with("vd") {
            Ok(DeviceType::SSD) // Virtual devices, assume SSD
        } else {
            Ok(DeviceType::Unknown)
        }
    }
    
    /// Update NUMA device mapping
    async fn update_numa_device_mapping(&self, devices: &HashMap<DeviceId, DeviceInfo>) -> Result<()> {
        for (device_id, device_info) in devices {
            let numa_node = self.detect_device_numa_node(&device_info.path).await?;
            
            if let Some(numa_node) = numa_node {
                self.device_to_numa.insert(device_id.clone(), numa_node);
                
                // Update NUMA topology with device mapping
                let mut topology = self.numa_topology.write();
                if let Some(node) = topology.nodes.get_mut(&numa_node) {
                    if !node.devices.contains(device_id) {
                        node.devices.push(device_id.clone());
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Detect which NUMA node a device belongs to
    async fn detect_device_numa_node(&self, device_path: &Path) -> Result<Option<u32>> {
        let device_name = device_path.file_name()
            .ok_or_else(|| anyhow!("Invalid device path"))?
            .to_string_lossy();
        
        // Try to find NUMA node from sysfs
        let numa_node_path = format!("/sys/block/{}/device/numa_node", device_name);
        
        if Path::new(&numa_node_path).exists() {
            let numa_str = tokio::fs::read_to_string(&numa_node_path).await?;
            let numa_node: i32 = numa_str.trim().parse()?;
            
            if numa_node >= 0 {
                Ok(Some(numa_node as u32))
            } else {
                Ok(None)
            }
        } else {
            // Fallback to node 0 for non-NUMA systems
            Ok(Some(0))
        }
    }
    
    /// Claim devices for a volume with intelligent selection
    pub async fn claim_devices_for_volume(
        &self,
        target_capacity: u64,
        performance_requirements: &PerformanceSpec,
    ) -> Result<Vec<DeviceId>> {
        info!("Claiming devices for volume: target_capacity={} bytes, requirements={:?}", 
              target_capacity, performance_requirements);
        
        let available_devices = self.available_devices.read().await;
        
        // Filter devices based on requirements
        let mut candidate_devices: Vec<_> = available_devices
            .values()
            .filter(|device| {
                !device.is_claimed &&
                performance_requirements.preferred_device_types.contains(&device.device_type)
            })
            .collect();
        
        // Sort by performance and NUMA locality
        candidate_devices.sort_by(|a, b| {
            // Prefer higher performance devices
            let a_perf = self.get_device_performance_score(a);
            let b_perf = self.get_device_performance_score(b);
            
            b_perf.partial_cmp(&a_perf).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Select devices to meet capacity requirements
        let mut selected_devices = Vec::new();
        let mut total_capacity = 0u64;
        
        for device in candidate_devices {
            if total_capacity >= target_capacity {
                break;
            }
            
            if selected_devices.len() >= self.config.max_devices_per_node {
                warn!("Reached maximum devices per node limit");
                break;
            }
            
            // Claim the device
            match self.claim_device_exclusive(device.device_id.clone()).await {
                Ok(()) => {
                    selected_devices.push(device.device_id.clone());
                    total_capacity += device.capacity;
                    info!("Claimed device: {} (capacity: {} bytes)", 
                          device.device_id, device.capacity);
                }
                Err(e) => {
                    warn!("Failed to claim device {}: {}", device.device_id, e);
                }
            }
        }
        
        if total_capacity < target_capacity {
            warn!("Could not meet full capacity requirement: got {} bytes, needed {} bytes",
                  total_capacity, target_capacity);
        }
        
        info!("Successfully claimed {} devices with total capacity {} bytes", 
              selected_devices.len(), total_capacity);
        
        Ok(selected_devices)
    }
    
    /// Get performance score for device selection
    fn get_device_performance_score(&self, device: &DeviceInfo) -> f64 {
        match device.device_type {
            DeviceType::NVMe => 1000.0,
            DeviceType::Optane => 950.0,
            DeviceType::SSD => 500.0,
            DeviceType::HDD => 100.0,
            DeviceType::Memory => 2000.0,
            DeviceType::Unknown => 10.0,
        }
    }
    
    /// Claim a device exclusively using flock
    pub async fn claim_device_exclusive(&self, device_id: DeviceId) -> Result<()> {
        let available_devices = self.available_devices.read().await;
        let device_info = available_devices.get(&device_id)
            .ok_or_else(|| anyhow!("Device not found: {}", device_id))?;
        
        // Create lock file
        let lock_file_path = self.config.lock_file_dir.join(format!("{}.lock", device_id));
        
        // Open device with O_DIRECT for exclusive access
        let device_file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_DIRECT | libc::O_EXCL)
            .open(&device_info.path)
            .context(format!("Failed to open device {} with O_DIRECT", device_info.path.display()))?;
        
        // Create and lock the lock file
        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&lock_file_path)
            .context("Failed to create lock file")?;
        
        // Acquire exclusive lock
        flock(lock_file.as_raw_fd(), FlockArg::LockExclusiveNonblock)
            .context("Failed to acquire exclusive lock on device")?;
        
        // Get device capacity and other details
        let (capacity, block_size) = self.get_device_capacity(&device_info.path).await?;
        let numa_node = self.device_to_numa.get(&device_id).map(|n| *n);
        
        // Get SMART data
        let smart_data = self.collect_smart_data(&device_info.path).await?;
        
        let claimed_device = ClaimedDevice {
            device_id: device_id.clone(),
            device_path: device_info.path.clone(),
            device_name: device_info.name.clone(),
            total_capacity: capacity,
            available_capacity: capacity, // Initially all available
            block_size,
            numa_node,
            device_type: device_info.device_type,
            smart_data,
            claimed_at: chrono::Utc::now(),
            lock_file: lock_file_path,
            performance_class: self.get_performance_class(device_info.device_type),
        };
        
        // Store the claimed device
        self.claimed_devices.insert(device_id.clone(), claimed_device);
        
        // Initialize health monitoring
        let health_monitor = HealthMonitor {
            device_id: device_id.clone(),
            last_check: Instant::now(),
            check_interval: self.config.health_check_interval,
            smart_data: self.collect_smart_data(&device_info.path).await?,
            performance_metrics: PerformanceMetrics {
                read_iops: 0.0,
                write_iops: 0.0,
                read_bandwidth: 0.0,
                write_bandwidth: 0.0,
                avg_latency_us: 0.0,
                queue_depth: 0,
                utilization_percent: 0.0,
            },
        };
        
        self.device_health.insert(device_id, health_monitor);
        
        // Keep the files open to maintain locks
        std::mem::forget(device_file);
        std::mem::forget(lock_file);
        
        Ok(())
    }
    
    /// Collect SMART data for predictive failure detection
    async fn collect_smart_data(&self, device_path: &Path) -> Result<SmartData> {
        // This would interface with smartctl or similar tools
        // For now, we'll return placeholder data
        Ok(SmartData {
            temperature: Some(45),
            power_on_hours: Some(1000),
            power_cycle_count: Some(100),
            reallocated_sectors: Some(0),
            pending_sectors: Some(0),
            offline_uncorrectable: Some(0),
            wear_leveling_count: Some(100),
            last_updated: chrono::Utc::now(),
            health_status: HealthStatus::Good,
        })
    }
    
    /// Get performance class for device type
    fn get_performance_class(&self, device_type: DeviceType) -> PerformanceClass {
        match device_type {
            DeviceType::NVMe | DeviceType::Optane | DeviceType::Memory => PerformanceClass::UltraHigh,
            DeviceType::SSD => PerformanceClass::High,
            DeviceType::HDD => PerformanceClass::Low,
            DeviceType::Unknown => PerformanceClass::Medium,
        }
    }
    
    /// Release a claimed device
    pub async fn release_device(&self, device_id: &DeviceId) -> Result<()> {
        if let Some((_, claimed_device)) = self.claimed_devices.remove(device_id) {
            // Remove lock file
            if let Err(e) = tokio::fs::remove_file(&claimed_device.lock_file).await {
                warn!("Failed to remove lock file {}: {}", 
                      claimed_device.lock_file.display(), e);
            }
            
            // Remove health monitoring
            self.device_health.remove(device_id);
            
            info!("Released device: {}", device_id);
        }
        
        Ok(())
    }
    
    /// Get claimed devices
    pub fn get_claimed_devices(&self) -> Vec<ClaimedDevice> {
        self.claimed_devices
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }
    
    /// Get available devices
    pub async fn get_available_devices(&self) -> Vec<DeviceInfo> {
        let available = self.available_devices.read().await;
        available.values().cloned().collect()
    }
    
    /// Get NUMA topology information
    pub fn get_numa_topology(&self) -> NumaTopology {
        self.numa_topology.read().clone()
    }
    
    /// Start periodic device discovery
    pub async fn start_periodic_discovery(&self) -> Result<()> {
        let discovery_interval = self.config.device_discovery_interval;
        let manager = self.clone();
        
        tokio::spawn(async move {
            loop {
                sleep(discovery_interval).await;
                
                if let Err(e) = manager.discover_devices().await {
                    error!("Periodic device discovery failed: {}", e);
                }
            }
        });
        
        Ok(())
    }
    
    /// Start health monitoring
    pub async fn start_health_monitoring(&self) -> Result<()> {
        let health_interval = self.config.health_check_interval;
        let manager = self.clone();
        
        tokio::spawn(async move {
            loop {
                sleep(health_interval).await;
                
                if let Err(e) = manager.check_device_health().await {
                    error!("Device health check failed: {}", e);
                }
            }
        });
        
        Ok(())
    }
    
    /// Check health of all claimed devices
    async fn check_device_health(&self) -> Result<()> {
        for entry in self.device_health.iter() {
            let device_id = entry.key();
            let mut health_monitor = entry.value().clone();
            
            if health_monitor.last_check.elapsed() >= health_monitor.check_interval {
                // Update SMART data
                if let Some(claimed_device) = self.claimed_devices.get(device_id) {
                    match self.collect_smart_data(&claimed_device.device_path).await {
                        Ok(smart_data) => {
                            health_monitor.smart_data = smart_data;
                            health_monitor.last_check = Instant::now();
                            
                            // Check for health issues
                            if health_monitor.smart_data.health_status == HealthStatus::Critical {
                                error!("Device {} is in critical state!", device_id);
                            } else if health_monitor.smart_data.health_status == HealthStatus::Warning {
                                warn!("Device {} is showing warning signs", device_id);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to collect SMART data for device {}: {}", device_id, e);
                        }
                    }
                }
                
                // Update the health monitor
                self.device_health.insert(device_id.clone(), health_monitor);
            }
        }
        
        Ok(())
    }
}

impl Clone for MultiDeviceManager {
    fn clone(&self) -> Self {
        Self {
            claimed_devices: self.claimed_devices.clone(),
            device_health: self.device_health.clone(),
            numa_topology: self.numa_topology.clone(),
            device_to_numa: self.device_to_numa.clone(),
            available_devices: self.available_devices.clone(),
            system: self.system.clone(),
            config: self.config.clone(),
            node_id: self.node_id.clone(),
            discovery_running: self.discovery_running.clone(),
        }
    }
}