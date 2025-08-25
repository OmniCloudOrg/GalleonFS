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