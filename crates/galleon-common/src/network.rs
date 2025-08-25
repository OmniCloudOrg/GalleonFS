//! Network protocols and communication for GalleonFS

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Response, Status};
use crate::types::*;
use crate::error::Result;

/// Network protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkProtocol {
    pub protocol_version: u32,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
    pub max_message_size: u32,
    pub timeout_seconds: u32,
}

impl Default for NetworkProtocol {
    fn default() -> Self {
        Self {
            protocol_version: crate::GALLEON_PROTOCOL_VERSION,
            compression_enabled: true,
            encryption_enabled: true,
            max_message_size: 32 * 1024 * 1024, // 32MB
            timeout_seconds: 30,
        }
    }
}

/// High-performance network transport options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportType {
    /// Standard TCP transport
    Tcp,
    /// QUIC transport for low latency
    Quic,
    /// RDMA transport for ultra-low latency
    Rdma,
    /// Unix domain sockets for local IPC
    Unix,
}

/// Network address with transport type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAddress {
    pub address: SocketAddr,
    pub transport: TransportType,
}

/// gRPC client connection manager
pub struct GrpcClient {
    channel: Channel,
    config: NetworkProtocol,
}

impl GrpcClient {
    /// Create a new gRPC client connection
    pub async fn connect(address: SocketAddr, config: NetworkProtocol) -> Result<Self> {
        let endpoint = Endpoint::from_shared(format!("http://{}", address))?
            .timeout(std::time::Duration::from_secs(config.timeout_seconds as u64))
            .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
            .tcp_nodelay(true)
            .http2_keep_alive_interval(std::time::Duration::from_secs(30))
            .keep_alive_timeout(std::time::Duration::from_secs(5))
            .keep_alive_while_idle(true);

        let channel = endpoint.connect().await?;
        
        Ok(Self {
            channel,
            config,
        })
    }

    /// Get the underlying gRPC channel
    pub fn channel(&self) -> Channel {
        self.channel.clone()
    }
}

/// Network message for peer-to-peer communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Heartbeat message for node health monitoring
    Heartbeat {
        node_id: NodeId,
        timestamp: chrono::DateTime<chrono::Utc>,
        metrics: NodeMetrics,
    },
    /// Data replication message
    ReplicateData {
        volume_id: VolumeId,
        chunk_id: ChunkId,
        data: Vec<u8>,
        checksum: Vec<u8>,
    },
    /// Volume metadata synchronization
    SyncMetadata {
        volume_id: VolumeId,
        metadata: Volume,
    },
    /// Cluster membership changes
    MembershipUpdate {
        nodes: Vec<NodeInfo>,
        leader_id: Option<NodeId>,
    },
    /// Device status updates
    DeviceUpdate {
        device: Device,
    },
}

/// Node performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeMetrics {
    pub cpu_usage_percent: f32,
    pub memory_usage_bytes: u64,
    pub memory_total_bytes: u64,
    pub disk_usage_bytes: u64,
    pub disk_total_bytes: u64,
    pub network_rx_bytes_per_sec: u64,
    pub network_tx_bytes_per_sec: u64,
    pub iops_read: u32,
    pub iops_write: u32,
    pub latency_read_ms: f32,
    pub latency_write_ms: f32,
    pub queue_depth: u32,
}

/// RDMA connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdmaConfig {
    pub device_name: String,
    pub port: u8,
    pub gid_index: u8,
    pub max_inline_data: u32,
    pub max_send_wr: u32,
    pub max_recv_wr: u32,
    pub max_send_sge: u32,
    pub max_recv_sge: u32,
}

impl Default for RdmaConfig {
    fn default() -> Self {
        Self {
            device_name: "mlx5_0".to_string(),
            port: 1,
            gid_index: 0,
            max_inline_data: 64,
            max_send_wr: 1024,
            max_recv_wr: 1024,
            max_send_sge: 1,
            max_recv_sge: 1,
        }
    }
}

/// QUIC connection configuration for fast replication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicConfig {
    pub max_streams: u64,
    pub max_stream_data: u64,
    pub max_connection_data: u64,
    pub idle_timeout_ms: u64,
    pub keep_alive_interval_ms: u64,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            max_streams: 1000,
            max_stream_data: 16 * 1024 * 1024, // 16MB
            max_connection_data: 1024 * 1024 * 1024, // 1GB
            idle_timeout_ms: 30000,
            keep_alive_interval_ms: 10000,
        }
    }
}

/// Network topology information for placement optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTopology {
    pub datacenter: String,
    pub zone: String,
    pub rack: String,
    pub switch: String,
    pub bandwidth_gbps: u32,
    pub latency_to_peers: std::collections::HashMap<NodeId, f32>,
}

/// Network encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEncryption {
    pub enabled: bool,
    pub algorithm: String,
    pub key_size: u32,
    pub certificate_path: Option<std::path::PathBuf>,
    pub private_key_path: Option<std::path::PathBuf>,
}

impl Default for NetworkEncryption {
    fn default() -> Self {
        Self {
            enabled: true,
            algorithm: "TLS1.3".to_string(),
            key_size: 256,
            certificate_path: None,
            private_key_path: None,
        }
    }
}