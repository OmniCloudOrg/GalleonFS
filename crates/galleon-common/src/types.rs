//! Core data structures and types for GalleonFS

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Unique identifier for a storage node
pub type NodeId = Uuid;

/// Unique identifier for a volume
pub type VolumeId = Uuid;

/// Unique identifier for a device
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
    
    pub fn from_path(path: &std::path::Path) -> Self {
        Self(path.to_string_lossy().into_owned())
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for DeviceId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for DeviceId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl DeviceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    
    pub fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        // This is a placeholder - real implementation would maintain fd mapping
        0
    }
}

/// Unique identifier for a chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkId(Uuid);

impl ChunkId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    pub fn from_volume_and_index(volume_id: VolumeId, index: u64) -> Self {
        // Create deterministic chunk ID from volume ID and index
        let mut bytes = volume_id.as_bytes().to_vec();
        bytes.extend_from_slice(&index.to_le_bytes());
        
        // Use a hash to create UUID
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let hash = hasher.finish();
        
        // Create UUID from hash (simplified)
        let uuid_bytes = [
            (hash >> 56) as u8, (hash >> 48) as u8, (hash >> 40) as u8, (hash >> 32) as u8,
            (hash >> 24) as u8, (hash >> 16) as u8, (hash >> 8) as u8, hash as u8,
            0, 0, 0, 0, 0, 0, 0, 0,
        ];
        
        Self(Uuid::from_bytes(uuid_bytes))
    }
}

impl std::fmt::Display for ChunkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for I/O buffers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BufferId(Uuid);

impl BufferId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for BufferId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// NUMA node identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NumaNode(pub usize);

impl std::fmt::Display for NumaNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "numa{}", self.0)
    }
}

/// Device type classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceType {
    NVMe,
    SATA,
    SAS,
    PATA,
    Unknown,
}

/// Device status enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceStatus {
    Healthy,
    Warning,
    Critical,
    Failed,
    Offline,
}

/// Device capabilities and characteristics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    pub supports_direct_io: bool,
    pub supports_trim: bool,
    pub supports_write_same: bool,
    pub max_io_size: usize,
    pub alignment_requirement: usize,
}

/// Comprehensive device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_path: PathBuf,
    pub capacity: u64,
    pub block_size: u32,
    pub capabilities: DeviceCapabilities,
    pub device_type: DeviceType,
    pub serial_number: Option<String>,
    pub model: Option<String>,
}

/// Device health monitoring data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceHealth {
    pub device_id: DeviceId,
    pub last_check: std::time::SystemTime,
    pub status: DeviceStatus,
    pub smart_data: Option<SmartData>,
    pub io_errors: u64,
    pub temperature: Option<u32>,
    pub wear_level: Option<u8>,
}

/// SMART data from storage devices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartData {
    pub temperature: Option<u32>,
    pub power_on_hours: Option<u64>,
    pub wear_level: Option<u8>,
    pub error_count: u64,
}

/// I/O operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IoOperationType {
    Read,
    Write,
    Sync,
    Trim,
}

/// I/O operation description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoOperation {
    pub device_id: DeviceId,
    pub operation_type: IoOperationType,
    pub offset: u64,
    pub size: usize,
    pub data: Option<Vec<u8>>,
}

/// Result of an I/O operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoResult {
    pub bytes_transferred: usize,
    pub latency_us: u64,
}

/// Volume specification for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSpec {
    pub volume_id: VolumeId,
    pub size: u64,
    pub chunk_size: Option<usize>,
    pub replication_factor: usize,
    pub erasure_coding: Option<ErasureCodingConfig>,
    pub performance_requirements: DeviceCapabilities,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
}

/// Erasure coding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErasureCodingConfig {
    pub data_blocks: usize,
    pub parity_blocks: usize,
}

/// Chunk specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSpec {
    pub chunk_id: ChunkId,
    pub volume_id: VolumeId,
    pub chunk_index: u64,
    pub size: usize,
    pub replication_factor: usize,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
}

/// Global storage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_operations: u64,
    pub total_bytes_read: u64,
    pub total_bytes_written: u64,
    pub average_latency_us: f64,
    pub active_volumes: usize,
    pub total_devices: usize,
    pub available_capacity: u64,
    pub used_capacity: u64,
}

/// Unique identifier for a chunk
pub type ChunkId = u64;

/// Block identifier within a volume
pub type BlockId = u64;

/// Storage node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: NodeId,
    pub address: SocketAddr,
    pub ipc_address: Option<SocketAddr>,
    pub datacenter: Option<String>,
    pub zone: Option<String>,
    pub rack: Option<String>,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub device_count: u32,
    pub status: NodeStatus,
    pub last_heartbeat: DateTime<Utc>,
    pub version: String,
}

/// Status of a storage node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeStatus {
    /// Node is healthy and operating normally
    Healthy,
    /// Node is under heavy load but still operational
    Degraded,
    /// Node is unreachable or not responding
    Unreachable,
    /// Node is being drained for maintenance
    Draining,
    /// Node is offline for maintenance
    Maintenance,
}

/// Volume configuration and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub id: VolumeId,
    pub name: Option<String>,
    pub size_bytes: u64,
    pub block_size: u64,
    pub chunk_size: u64,
    pub volume_type: VolumeType,
    pub storage_class: String,
    pub replication_factor: u8,
    pub erasure_coding: Option<ErasureCodingConfig>,
    pub encryption: Option<EncryptionConfig>,
    pub compression: Option<CompressionConfig>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub state: VolumeState,
    pub mount_points: Vec<MountPoint>,
    pub topology: VolumeTopology,
    pub metrics: VolumeMetrics,
}

/// Type of volume
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VolumeType {
    /// Temporary volume, deleted when unmounted
    Ephemeral,
    /// Persistent volume with data durability
    Persistent,
    /// Shared volume for multi-node access
    Shared,
}

/// Current state of a volume
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VolumeState {
    /// Volume is being created
    Creating,
    /// Volume is ready for use
    Available,
    /// Volume is being used
    InUse,
    /// Volume is being deleted
    Deleting,
    /// Volume has failed and needs recovery
    Failed,
    /// Volume is being migrated
    Migrating,
    /// Volume is being expanded
    Expanding,
}

/// Erasure coding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErasureCodingConfig {
    pub data_blocks: u8,
    pub parity_blocks: u8,
    pub algorithm: ErasureCodingAlgorithm,
}

/// Erasure coding algorithms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ErasureCodingAlgorithm {
    ReedSolomon,
    ReedSolomonSimd,
}

/// Encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    pub algorithm: EncryptionAlgorithm,
    pub key_id: String,
    pub key_rotation_interval: chrono::Duration,
}

/// Encryption algorithms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
    ChaCha20Poly1305,
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    pub algorithm: CompressionAlgorithm,
    pub level: u8,
    pub enabled: bool,
}

/// Compression algorithms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CompressionAlgorithm {
    Lz4,
    Zstd,
    Snappy,
}

/// Mount point information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountPoint {
    pub path: PathBuf,
    pub node_id: NodeId,
    pub options: Vec<String>,
    pub mounted_at: DateTime<Utc>,
}

/// Real-time volume metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VolumeMetrics {
    pub read_iops: f64,
    pub write_iops: f64,
    pub read_throughput_bytes_per_sec: f64,
    pub write_throughput_bytes_per_sec: f64,
    pub read_latency_ms: f64,
    pub write_latency_ms: f64,
    pub queue_depth: u32,
    pub cache_hit_ratio: f64,
    pub error_rate: f64,
}

/// Device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    pub node_id: NodeId,
    pub path: PathBuf,
    pub device_type: DeviceType,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub status: DeviceStatus,
    pub health: DeviceHealth,
    pub performance_tier: PerformanceTier,
    pub numa_node: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub last_check: DateTime<Utc>,
}

/// Device operational status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceStatus {
    /// Device is healthy and available
    Online,
    /// Device is degraded but functional
    Degraded,
    /// Device has failed
    Failed,
    /// Device is being replaced
    Replacing,
    /// Device is in maintenance mode
    Maintenance,
}

/// Device health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceHealth {
    pub smart_status: SmartStatus,
    pub temperature_celsius: Option<f32>,
    pub power_cycles: Option<u64>,
    pub power_on_hours: Option<u64>,
    pub wear_level: Option<f32>, // 0.0 to 1.0
    pub bad_blocks: Option<u64>,
    pub predicted_failure: bool,
}

/// SMART status from device
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SmartStatus {
    Passed,
    Failed,
    Unknown,
}

/// Performance tier for storage devices
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PerformanceTier {
    /// Ultra-high performance (NVMe, Optane)
    UltraFast = 0,
    /// High performance (SSD)
    Fast = 1,
    /// Medium performance (Hybrid)
    Medium = 2,
    /// Standard performance (HDD)
    Standard = 3,
    /// Archive storage (Cold HDD)
    Archive = 4,
}

/// Chunk metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub id: ChunkId,
    pub volume_id: VolumeId,
    pub offset: u64,
    pub size: u64,
    pub checksum: Vec<u8>,
    pub compression_ratio: Option<f32>,
    pub replicas: Vec<ChunkReplica>,
    pub created_at: DateTime<Utc>,
    pub accessed_at: DateTime<Utc>,
}

/// Information about a chunk replica
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkReplica {
    pub node_id: NodeId,
    pub device_id: DeviceId,
    pub physical_offset: u64,
    pub status: ReplicaStatus,
}

/// Status of a chunk replica
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReplicaStatus {
    /// Replica is healthy and current
    Healthy,
    /// Replica is stale and needs sync
    Stale,
    /// Replica is corrupted
    Corrupted,
    /// Replica is being created
    Creating,
    /// Replica is being deleted
    Deleting,
}

/// Write consistency level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WriteConcern {
    /// Acknowledged after primary write
    WriteAcknowledged,
    /// Acknowledged after flush to disk
    WriteDurable,
    /// Acknowledged after replication to 1+ nodes
    WriteReplicated,
    /// Acknowledged after cross-zone replication
    WriteDistributed,
}

/// Storage class definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageClass {
    pub name: String,
    pub provisioner: String,
    pub parameters: HashMap<String, String>,
    pub reclaim_policy: ReclaimPolicy,
    pub volume_binding_mode: VolumeBindingMode,
    pub allowed_topologies: Vec<String>,
    pub mount_options: Vec<String>,
}

/// Volume reclaim policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReclaimPolicy {
    /// Delete volume when no longer needed
    Delete,
    /// Retain volume for manual cleanup
    Retain,
}

/// Volume binding mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VolumeBindingMode {
    /// Bind immediately when volume is created
    Immediate,
    /// Wait for first consumer before binding
    WaitForFirstConsumer,
}

/// Device type classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeviceType {
    /// NVMe SSD device
    NVMe,
    /// SATA/SAS SSD device  
    SSD,
    /// Traditional hard disk drive
    HDD,
}

/// Device capacity information
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceCapacity {
    bytes: u64,
}

impl DeviceCapacity {
    pub fn new(bytes: u64) -> Self {
        Self { bytes }
    }
    
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    
    pub fn kb(&self) -> u64 {
        self.bytes / 1024
    }
    
    pub fn mb(&self) -> u64 {
        self.bytes / (1024 * 1024)
    }
    
    pub fn gb(&self) -> u64 {
        self.bytes / (1024 * 1024 * 1024)
    }
    
    pub fn tb(&self) -> u64 {
        self.bytes / (1024 * 1024 * 1024 * 1024)
    }
}

/// QoS (Quality of Service) policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QoSPolicy {
    pub name: String,
    pub limits: QoSLimits,
    pub guarantees: QoSGuarantees,
    pub burstable: Option<BurstableQoS>,
}

/// QoS resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QoSLimits {
    pub iops: Option<u32>,
    pub throughput_mbps: Option<u32>,
    pub latency_ms: Option<u32>,
}

/// QoS resource guarantees
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QoSGuarantees {
    pub iops: Option<u32>,
    pub throughput_mbps: Option<u32>,
}

/// Burstable QoS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurstableQoS {
    pub enabled: bool,
    pub duration_seconds: u32,
    pub iops_multiplier: f32,
    pub throughput_multiplier: f32,
}