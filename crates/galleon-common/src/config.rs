//! Configuration management for GalleonFS

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use crate::types::*;
use crate::network::*;
use crate::error::{Result, GalleonError};

/// Main configuration for GalleonFS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleonConfig {
    /// Node configuration
    pub node: NodeConfig,
    /// Storage configuration
    pub storage: StorageConfig,
    /// Network configuration
    pub network: NetworkConfig,
    /// Cluster configuration
    pub cluster: ClusterConfig,
    /// Security configuration
    pub security: SecurityConfig,
    /// Performance configuration
    pub performance: PerformanceConfig,
    /// Observability configuration
    pub observability: ObservabilityConfig,
}

/// Node-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node ID (generated if not specified)
    pub id: Option<NodeId>,
    /// Datacenter name
    pub datacenter: Option<String>,
    /// Availability zone
    pub zone: Option<String>,
    /// Rack identifier
    pub rack: Option<String>,
    /// Node role
    pub role: NodeRole,
    /// Resource limits
    pub resources: ResourceLimits,
}

/// Node role in the cluster
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeRole {
    /// Storage node that hosts data
    Storage,
    /// Coordinator node for cluster management
    Coordinator,
    /// Gateway node for external access
    Gateway,
    /// Metrics collection node
    Metrics,
    /// Combined node with multiple roles
    Combined(Vec<NodeRole>),
}

/// Resource limits for the node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory usage in bytes
    pub max_memory_bytes: Option<u64>,
    /// Maximum CPU cores to use
    pub max_cpu_cores: Option<u32>,
    /// Maximum network bandwidth in bytes/sec
    pub max_network_bandwidth_bps: Option<u64>,
    /// Maximum disk I/O bandwidth in bytes/sec
    pub max_disk_bandwidth_bps: Option<u64>,
}

/// Journaling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalConfig {
    /// Enable write-ahead journaling
    pub enabled: bool,
    /// Journal size per device
    pub size_per_device: u64,
    /// Sync interval for journal writes
    pub sync_interval: chrono::Duration,
    /// Checkpoint interval
    pub checkpoint_interval: chrono::Duration,
}

/// Caching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable caching
    pub enabled: bool,
    /// Memory cache size in bytes
    pub memory_cache_size: u64,
    /// SSD cache size in bytes (if available)
    pub ssd_cache_size: Option<u64>,
    /// Cache replacement policy
    pub replacement_policy: CacheReplacementPolicy,
    /// Cache write policy
    pub write_policy: CacheWritePolicy,
}

/// Cache replacement policies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CacheReplacementPolicy {
    /// Least Recently Used
    Lru,
    /// Least Frequently Used
    Lfu,
    /// Adaptive Replacement Cache
    Arc,
    /// Clock algorithm
    Clock,
}

/// Cache write policies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CacheWritePolicy {
    /// Write through to storage immediately
    WriteThrough,
    /// Write back to storage later
    WriteBack,
    /// Write around (bypass cache)
    WriteAround,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Bind address for storage service
    pub bind_address: SocketAddr,
    /// IPC address for CLI communication
    pub ipc_address: SocketAddr,
    /// Network protocol configuration
    pub protocol: NetworkProtocol,
    /// Transport configuration
    pub transport: TransportConfig,
    /// Load balancing configuration
    pub load_balancing: LoadBalancingConfig,
}

/// Transport layer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Preferred transport type
    pub preferred_transport: TransportType,
    /// TCP configuration
    pub tcp: TcpConfig,
    /// QUIC configuration
    pub quic: Option<QuicConfig>,
    /// RDMA configuration
    pub rdma: Option<RdmaConfig>,
}

/// TCP transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpConfig {
    /// TCP keep-alive interval
    pub keep_alive_interval: chrono::Duration,
    /// TCP no-delay option
    pub no_delay: bool,
    /// Receive buffer size
    pub recv_buffer_size: Option<u32>,
    /// Send buffer size
    pub send_buffer_size: Option<u32>,
}

/// Load balancing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancingConfig {
    /// Load balancing algorithm
    pub algorithm: LoadBalancingAlgorithm,
    /// Health check configuration
    pub health_check: HealthCheckConfig,
}

/// Load balancing algorithms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoadBalancingAlgorithm {
    /// Round robin
    RoundRobin,
    /// Least connections
    LeastConnections,
    /// Weighted round robin
    WeightedRoundRobin,
    /// Consistent hashing
    ConsistentHashing,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check interval
    pub interval: chrono::Duration,
    /// Health check timeout
    pub timeout: chrono::Duration,
    /// Number of failed checks before marking unhealthy
    pub failure_threshold: u32,
    /// Number of successful checks before marking healthy
    pub success_threshold: u32,
}

/// Cluster management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Cluster name
    pub name: String,
    /// Peer addresses for joining existing cluster
    pub peer_addresses: Vec<SocketAddr>,
    /// Auto-join existing cluster if found
    pub auto_join: bool,
    /// Leader election configuration
    pub leader_election: LeaderElectionConfig,
    /// Membership configuration
    pub membership: MembershipConfig,
}

/// Leader election configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderElectionConfig {
    /// Election timeout
    pub election_timeout: chrono::Duration,
    /// Heartbeat interval
    pub heartbeat_interval: chrono::Duration,
    /// Maximum number of election retries
    pub max_retries: u32,
}

/// Membership management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipConfig {
    /// Gossip interval for membership updates
    pub gossip_interval: chrono::Duration,
    /// Failure detection timeout
    pub failure_timeout: chrono::Duration,
    /// Maximum cluster size
    pub max_cluster_size: u32,
    /// Minimum cluster size for operations
    pub min_cluster_size: u32,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Encryption configuration
    pub encryption: EncryptionSecurityConfig,
    /// Authentication configuration
    pub authentication: AuthenticationConfig,
    /// Authorization configuration
    pub authorization: AuthorizationConfig,
    /// Audit logging configuration
    pub audit: AuditConfig,
}

/// Encryption security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionSecurityConfig {
    /// Enable encryption at rest
    pub at_rest: bool,
    /// Enable encryption in transit
    pub in_transit: bool,
    /// Key management configuration
    pub key_management: KeyManagementConfig,
}

/// Key management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyManagementConfig {
    /// Key management provider
    pub provider: KeyManagementProvider,
    /// Key rotation interval
    pub rotation_interval: chrono::Duration,
    /// External key management service configuration
    pub external_kms: Option<ExternalKmsConfig>,
}

/// Key management providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KeyManagementProvider {
    /// Internal key management
    Internal,
    /// External KMS service
    External,
    /// Hardware Security Module
    Hsm,
}

/// External KMS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalKmsConfig {
    /// KMS service URL
    pub url: String,
    /// Authentication credentials
    pub credentials: KmsCredentials,
}

/// KMS authentication credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KmsCredentials {
    /// Access key ID
    pub access_key_id: String,
    /// Secret access key
    pub secret_access_key: String,
    /// Region (if applicable)
    pub region: Option<String>,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationConfig {
    /// Enable authentication
    pub enabled: bool,
    /// Authentication provider
    pub provider: AuthenticationProvider,
    /// Token expiration time
    pub token_expiration: chrono::Duration,
}

/// Authentication providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuthenticationProvider {
    /// No authentication
    None,
    /// Local user database
    Local,
    /// LDAP/Active Directory
    Ldap,
    /// OAuth2/OIDC
    OAuth2,
    /// Certificate-based authentication
    Certificate,
}

/// Authorization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationConfig {
    /// Enable authorization
    pub enabled: bool,
    /// Default permissions for new users
    pub default_permissions: Vec<Permission>,
    /// Admin users with full access
    pub admin_users: Vec<String>,
}

/// Permission types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Permission {
    /// Read access to volumes
    VolumeRead,
    /// Write access to volumes
    VolumeWrite,
    /// Volume management (create, delete)
    VolumeManage,
    /// Cluster administration
    ClusterAdmin,
    /// Metrics access
    MetricsRead,
}

/// Audit logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Enable audit logging
    pub enabled: bool,
    /// Audit log file path
    pub log_file: Option<PathBuf>,
    /// Events to audit
    pub events: Vec<AuditEvent>,
}

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuditEvent {
    /// Volume operations
    VolumeOperation,
    /// Authentication events
    Authentication,
    /// Authorization events
    Authorization,
    /// Configuration changes
    ConfigurationChange,
    /// System events
    SystemEvent,
}

/// Performance tuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// I/O configuration
    pub io: IoConfig,
    /// Memory configuration
    pub memory: MemoryConfig,
    /// CPU configuration
    pub cpu: CpuConfig,
    /// QoS configuration
    pub qos: QoSConfig,
}

/// I/O performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoConfig {
    /// Enable io_uring
    pub use_io_uring: bool,
    /// Number of I/O rings per device
    pub rings_per_device: u32,
    /// I/O queue depth
    pub queue_depth: u32,
    /// Use O_DIRECT for bypassing page cache
    pub use_direct_io: bool,
    /// Use huge pages for I/O buffers
    pub use_huge_pages: bool,
}

/// Memory performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Use jemalloc allocator
    pub use_jemalloc: bool,
    /// Enable NUMA awareness
    pub numa_aware: bool,
    /// Memory pool size for I/O operations
    pub io_pool_size: u64,
    /// Memory prefaulting
    pub prefault_memory: bool,
}

/// CPU performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuConfig {
    /// CPU affinity for I/O threads
    pub io_thread_affinity: Option<Vec<u32>>,
    /// CPU affinity for network threads
    pub network_thread_affinity: Option<Vec<u32>>,
    /// Enable CPU frequency scaling
    pub enable_frequency_scaling: bool,
}

/// QoS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QoSConfig {
    /// Default QoS policies
    pub default_policies: HashMap<String, QoSPolicy>,
    /// Enable QoS enforcement
    pub enforcement_enabled: bool,
    /// QoS monitoring interval
    pub monitoring_interval: chrono::Duration,
}

/// Observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Metrics configuration
    pub metrics: MetricsConfig,
    /// Tracing configuration
    pub tracing: TracingConfig,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: LogLevel,
    /// Log format
    pub format: LogFormat,
    /// Log output destination
    pub output: LogOutput,
    /// Structured logging fields
    pub fields: Vec<String>,
}

/// Log levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Log output formats
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogFormat {
    /// Human-readable text format
    Text,
    /// JSON format for structured logging
    Json,
    /// Compact format
    Compact,
}

/// Log output destinations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogOutput {
    /// Standard output
    Stdout,
    /// Standard error
    Stderr,
    /// File output
    File(PathBuf),
    /// Syslog output
    Syslog,
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    /// Metrics collection interval
    pub collection_interval: chrono::Duration,
    /// Prometheus metrics endpoint
    pub prometheus_endpoint: Option<SocketAddr>,
    /// Custom metrics
    pub custom_metrics: Vec<String>,
}

/// Distributed tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Enable distributed tracing
    pub enabled: bool,
    /// Tracing service endpoint
    pub endpoint: Option<String>,
    /// Sampling rate (0.0 to 1.0)
    pub sampling_rate: f64,
    /// Service name for tracing
    pub service_name: String,
}

impl Default for GalleonConfig {
    fn default() -> Self {
        Self {
            node: NodeConfig::default(),
            storage: StorageConfig::default(),
            network: NetworkConfig::default(),
            cluster: ClusterConfig::default(),
            security: SecurityConfig::default(),
            performance: PerformanceConfig::default(),
            observability: ObservabilityConfig::default(),
        }
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            id: None,
            datacenter: None,
            zone: None,
            rack: None,
            role: NodeRole::Combined(vec![
                NodeRole::Storage,
                NodeRole::Coordinator,
                NodeRole::Gateway,
            ]),
            resources: ResourceLimits::default(),
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: None,
            max_cpu_cores: None,
            max_network_bandwidth_bps: None,
            max_disk_bandwidth_bps: None,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./galleonfs_storage"),
            block_size: crate::DEFAULT_BLOCK_SIZE,
            chunk_size: crate::DEFAULT_CHUNK_SIZE,
            devices: DeviceConfig::default(),
            storage_classes: HashMap::new(),
            journal: JournalConfig::default(),
            cache: CacheConfig::default(),
        }
    }
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            auto_discovery: true,
            device_paths: Vec::new(),
            exclude_patterns: vec![
                "loop*".to_string(),
                "ram*".to_string(),
                "dm-*".to_string(),
            ],
            min_device_size: 1024 * 1024 * 1024, // 1GB
            health_check_interval: chrono::Duration::minutes(5),
        }
    }
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            size_per_device: 256 * 1024 * 1024, // 256MB
            sync_interval: chrono::Duration::seconds(1),
            checkpoint_interval: chrono::Duration::minutes(5),
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            memory_cache_size: 1024 * 1024 * 1024, // 1GB
            ssd_cache_size: None,
            replacement_policy: CacheReplacementPolicy::Lru,
            write_policy: CacheWritePolicy::WriteThrough,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".parse().unwrap(),
            ipc_address: "127.0.0.1:8090".parse().unwrap(),
            protocol: NetworkProtocol::default(),
            transport: TransportConfig::default(),
            load_balancing: LoadBalancingConfig::default(),
        }
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            preferred_transport: TransportType::Tcp,
            tcp: TcpConfig::default(),
            quic: None,
            rdma: None,
        }
    }
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self {
            keep_alive_interval: chrono::Duration::seconds(30),
            no_delay: true,
            recv_buffer_size: None,
            send_buffer_size: None,
        }
    }
}

impl Default for LoadBalancingConfig {
    fn default() -> Self {
        Self {
            algorithm: LoadBalancingAlgorithm::RoundRobin,
            health_check: HealthCheckConfig::default(),
        }
    }
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: chrono::Duration::seconds(30),
            timeout: chrono::Duration::seconds(5),
            failure_threshold: 3,
            success_threshold: 2,
        }
    }
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            name: "galleonfs-cluster".to_string(),
            peer_addresses: Vec::new(),
            auto_join: true,
            leader_election: LeaderElectionConfig::default(),
            membership: MembershipConfig::default(),
        }
    }
}

impl Default for LeaderElectionConfig {
    fn default() -> Self {
        Self {
            election_timeout: chrono::Duration::seconds(10),
            heartbeat_interval: chrono::Duration::seconds(2),
            max_retries: 3,
        }
    }
}

impl Default for MembershipConfig {
    fn default() -> Self {
        Self {
            gossip_interval: chrono::Duration::seconds(1),
            failure_timeout: chrono::Duration::seconds(30),
            max_cluster_size: crate::MAX_CLUSTER_NODES,
            min_cluster_size: 1,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            encryption: EncryptionSecurityConfig::default(),
            authentication: AuthenticationConfig::default(),
            authorization: AuthorizationConfig::default(),
            audit: AuditConfig::default(),
        }
    }
}

impl Default for EncryptionSecurityConfig {
    fn default() -> Self {
        Self {
            at_rest: true,
            in_transit: false,
            key_management: KeyManagementConfig::default(),
        }
    }
}

impl Default for KeyManagementConfig {
    fn default() -> Self {
        Self {
            provider: KeyManagementProvider::Internal,
            rotation_interval: chrono::Duration::days(30),
            external_kms: None,
        }
    }
}

impl Default for AuthenticationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: AuthenticationProvider::None,
            token_expiration: chrono::Duration::hours(24),
        }
    }
}

impl Default for AuthorizationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_permissions: vec![
                Permission::VolumeRead,
                Permission::VolumeWrite,
                Permission::MetricsRead,
            ],
            admin_users: Vec::new(),
        }
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            log_file: None,
            events: vec![
                AuditEvent::VolumeOperation,
                AuditEvent::Authentication,
                AuditEvent::Authorization,
            ],
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            io: IoConfig::default(),
            memory: MemoryConfig::default(),
            cpu: CpuConfig::default(),
            qos: QoSConfig::default(),
        }
    }
}

impl Default for IoConfig {
    fn default() -> Self {
        Self {
            use_io_uring: true,
            rings_per_device: 1,
            queue_depth: 128,
            use_direct_io: true,
            use_huge_pages: false,
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            use_jemalloc: true,
            numa_aware: true,
            io_pool_size: 256 * 1024 * 1024, // 256MB
            prefault_memory: false,
        }
    }
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            io_thread_affinity: None,
            network_thread_affinity: None,
            enable_frequency_scaling: true,
        }
    }
}

impl Default for QoSConfig {
    fn default() -> Self {
        Self {
            default_policies: HashMap::new(),
            enforcement_enabled: true,
            monitoring_interval: chrono::Duration::seconds(10),
        }
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            logging: LoggingConfig::default(),
            metrics: MetricsConfig::default(),
            tracing: TracingConfig::default(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            format: LogFormat::Text,
            output: LogOutput::Stdout,
            fields: vec![
                "timestamp".to_string(),
                "level".to_string(),
                "target".to_string(),
                "message".to_string(),
            ],
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval: chrono::Duration::seconds(10),
            prometheus_endpoint: None,
            custom_metrics: Vec::new(),
        }
    }
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            sampling_rate: 0.1,
            service_name: "galleonfs".to_string(),
        }
    }
}

/// Configuration loader that supports multiple formats
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from file
    pub fn load_from_file(path: &std::path::Path) -> Result<GalleonConfig> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| GalleonError::ConfigError(format!("Failed to read config file: {}", e)))?;

        match path.extension().and_then(|s| s.to_str()) {
            Some("toml") => Self::load_from_toml(&content),
            Some("yaml") | Some("yml") => Self::load_from_yaml(&content),
            Some("json") => Self::load_from_json(&content),
            _ => Err(GalleonError::ConfigError("Unsupported config file format".to_string())),
        }
    }

    /// Load configuration from TOML string
    pub fn load_from_toml(content: &str) -> Result<GalleonConfig> {
        toml::from_str(content)
            .map_err(|e| GalleonError::ConfigError(format!("Failed to parse TOML config: {}", e)))
    }

    /// Load configuration from YAML string
    pub fn load_from_yaml(content: &str) -> Result<GalleonConfig> {
        serde_yaml::from_str(content)
            .map_err(|e| GalleonError::ConfigError(format!("Failed to parse YAML config: {}", e)))
    }

    /// Load configuration from JSON string
    pub fn load_from_json(content: &str) -> Result<GalleonConfig> {
        serde_json::from_str(content)
            .map_err(|e| GalleonError::ConfigError(format!("Failed to parse JSON config: {}", e)))
    }

    /// Save configuration to file
    pub fn save_to_file(config: &GalleonConfig, path: &std::path::Path) -> Result<()> {
        let content = match path.extension().and_then(|s| s.to_str()) {
            Some("toml") => Self::save_to_toml(config)?,
            Some("yaml") | Some("yml") => Self::save_to_yaml(config)?,
            Some("json") => Self::save_to_json(config)?,
            _ => return Err(GalleonError::ConfigError("Unsupported config file format".to_string())),
        };

        std::fs::write(path, content)
            .map_err(|e| GalleonError::ConfigError(format!("Failed to write config file: {}", e)))?;

        Ok(())
    }

    /// Save configuration to TOML string
    pub fn save_to_toml(config: &GalleonConfig) -> Result<String> {
        toml::to_string_pretty(config)
            .map_err(|e| GalleonError::ConfigError(format!("Failed to serialize TOML config: {}", e)))
    }

    /// Save configuration to YAML string
    pub fn save_to_yaml(config: &GalleonConfig) -> Result<String> {
        serde_yaml::to_string(config)
            .map_err(|e| GalleonError::ConfigError(format!("Failed to serialize YAML config: {}", e)))
    }

    /// Save configuration to JSON string
    pub fn save_to_json(config: &GalleonConfig) -> Result<String> {
        serde_json::to_string_pretty(config)
            .map_err(|e| GalleonError::ConfigError(format!("Failed to serialize JSON config: {}", e)))
    }
}

/// Storage node specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageNodeConfig {
    /// Node identification
    pub node_id: NodeId,
    pub bind_address: SocketAddr,
    pub ipc_address: Option<SocketAddr>,
    
    /// Device management
    pub max_devices_per_node: usize,
    pub enable_numa: bool,
    pub enable_hw_accel: bool,
    pub enable_cpu_affinity: bool,
    
    /// I/O engine settings
    pub io_workers_per_numa: usize,
    pub max_queue_depth: u32,
    
    /// Storage paths
    pub metadata_path: PathBuf,
    pub wal_path: PathBuf,
    
    /// Cache and memory
    pub cache_size_mb: usize,
    
    /// Features
    pub enable_compression: bool,
    pub enable_encryption: bool,
    
    /// Device management configuration
    pub device: DeviceConfig,
    
    /// I/O engine configuration  
    pub io_engine: IoEngineConfig,
    
    /// Storage engine configuration
    pub storage: StorageConfig,
    
    /// Metrics configuration
    pub metrics: MetricsConfig,
    
    /// Security configuration
    pub security: SecurityConfig,
}

/// Device management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// Maximum devices per node
    pub max_devices_per_node: usize,
    
    /// Minimum device capacity in bytes
    pub min_device_capacity: u64,
    
    /// Device discovery cache duration in seconds
    pub discovery_cache_seconds: u64,
    
    /// Device types to include
    pub allowed_device_types: Vec<String>,
    
    /// Device paths to exclude
    pub excluded_device_paths: Vec<String>,
    
    /// SMART monitoring enabled
    pub smart_monitoring_enabled: bool,
    
    /// Health check interval in seconds
    pub health_check_interval_seconds: u64,
}

/// I/O engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoEngineConfig {
    /// io_uring queue size
    pub ring_size: u32,
    
    /// Number of I/O threads per device
    pub threads_per_device: u32,
    
    /// Buffer pool configuration
    pub buffers_per_numa_node: usize,
    pub buffer_size: usize,
    
    /// Batch processing settings
    pub max_batch_size: u32,
    pub batch_timeout_ms: u64,
    
    /// Performance tuning
    pub use_sqpoll: bool,
    pub use_huge_pages: bool,
    pub numa_aware: bool,
}

/// Storage engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Chunk configuration
    pub chunk_size: u64,
    pub max_chunks_per_device: u64,
    
    /// Replication settings
    pub default_replication_factor: u32,
    pub enable_erasure_coding: bool,
    
    /// Compression settings
    pub enable_compression: bool,
    pub compression_algorithm: String,
    pub compression_level: u32,
    
    /// Encryption settings
    pub enable_encryption: bool,
    pub encryption_algorithm: String,
    
    /// WAL configuration
    pub wal_enabled: bool,
    pub wal_sync_interval_ms: u64,
}

impl Default for StorageNodeConfig {
    fn default() -> Self {
        Self {
            node_id: NodeId::new(&uuid::Uuid::new_v4().to_string()),
            bind_address: "0.0.0.0:9001".parse().unwrap(),
            ipc_address: None,
            max_devices_per_node: 1000,
            enable_numa: true,
            enable_hw_accel: true,
            enable_cpu_affinity: true,
            io_workers_per_numa: 2,
            max_queue_depth: 256,
            metadata_path: PathBuf::from("/var/lib/galleonfs/metadata"),
            wal_path: PathBuf::from("/var/lib/galleonfs/wal"),
            cache_size_mb: 1024,
            enable_compression: true,
            enable_encryption: true,
            device: DeviceConfig::default(),
            io_engine: IoEngineConfig::default(),
            storage: StorageConfig::default(),
            metrics: MetricsConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            max_devices_per_node: 1000,
            min_device_capacity: 1024 * 1024 * 1024, // 1GB
            discovery_cache_seconds: 60,
            allowed_device_types: vec!["nvme".to_string(), "ssd".to_string(), "hdd".to_string()],
            excluded_device_paths: vec![],
            smart_monitoring_enabled: true,
            health_check_interval_seconds: 30,
        }
    }
}

impl Default for IoEngineConfig {
    fn default() -> Self {
        Self {
            ring_size: 256,
            threads_per_device: 2,
            buffers_per_numa_node: 1024,
            buffer_size: 2 * 1024 * 1024, // 2MB
            max_batch_size: 64,
            batch_timeout_ms: 1,
            use_sqpoll: true,
            use_huge_pages: true,
            numa_aware: true,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            chunk_size: 64 * 1024 * 1024, // 64MB
            max_chunks_per_device: 1000000,
            default_replication_factor: 3,
            enable_erasure_coding: true,
            enable_compression: true,
            compression_algorithm: "zstd".to_string(),
            compression_level: 3,
            enable_encryption: true,
            encryption_algorithm: "aes-256-gcm".to_string(),
            wal_enabled: true,
            wal_sync_interval_ms: 100,
        }
    }
}

impl StorageNodeConfig {
    /// Load storage node configuration from environment and files
    pub async fn from_file(path: &str) -> Result<Self> {
        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| GalleonError::ConfigError(format!("Failed to read config file {}: {}", path, e)))?;
        
        if path.ends_with(".toml") {
            toml::from_str(&content)
                .map_err(|e| GalleonError::ConfigError(format!("Failed to parse TOML config: {}", e)))
        } else if path.ends_with(".yaml") || path.ends_with(".yml") {
            serde_yaml::from_str(&content)
                .map_err(|e| GalleonError::ConfigError(format!("Failed to parse YAML config: {}", e)))
        } else if path.ends_with(".json") {
            serde_json::from_str(&content)
                .map_err(|e| GalleonError::ConfigError(format!("Failed to parse JSON config: {}", e)))
        } else {
            Err(GalleonError::ConfigError("Unknown config file format".to_string()))
        }
    }
    
    /// Load storage node configuration from environment and files
    pub async fn load() -> Result<Self> {
        // Try to load from config file first
        if let Ok(config_path) = std::env::var("GALLEON_STORAGE_CONFIG") {
            return Self::from_file(&config_path).await;
        }
        
        // Default configuration with environment overrides
        let mut config = Self::default();
        
        // Override with environment variables
        if let Ok(node_id) = std::env::var("GALLEON_NODE_ID") {
            config.node_id = NodeId::new(&node_id);
        }
        
        if let Ok(listen_addr) = std::env::var("GALLEON_LISTEN_ADDRESS") {
            config.bind_address = listen_addr.parse()
                .map_err(|e| GalleonError::ConfigError(format!("Invalid listen address: {}", e)))?;
        }
        
        Ok(config)
    }
}