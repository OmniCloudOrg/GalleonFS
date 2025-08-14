//! GalleonFS Daemon Module
//! 
//! This module provides the daemon functionality for GalleonFS, which runs
//! the storage service persistently in the background and provides an IPC
//! interface for CLI clients to communicate with.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, error};
use uuid::Uuid;

use crate::{
    storage::FileStorageEngine,
    GalleonFS,
    PersistenceLevel,
    ReplicationStrategy,
    VolumeType,
    StorageClass,
    Volume,
};

/// Configuration for the GalleonFS daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Storage path for the daemon
    pub storage_path: PathBuf,
    /// Block size for storage operations
    pub block_size: u64,
    /// Address to bind the storage service to
    pub bind_address: String,
    /// Address to bind the IPC service to for CLI communication
    pub ipc_address: String,
    /// Peer addresses for cluster formation
    pub peer_addresses: Vec<String>,
    /// Replication strategy
    pub replication_strategy: ReplicationStrategy,
    /// Persistence level
    pub persistence_level: PersistenceLevel,
    /// Mount point for volumes (optional)
    pub mount_point: Option<PathBuf>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            storage_path: PathBuf::from("./galleonfs_storage"),
            block_size: 4096,
            bind_address: "127.0.0.1:8080".to_string(),
            ipc_address: "127.0.0.1:8090".to_string(),
            peer_addresses: Vec::new(),
            replication_strategy: ReplicationStrategy::Synchronous,
            persistence_level: PersistenceLevel::Enhanced,
            mount_point: None,
        }
    }
}

/// IPC message types for communication between daemon and CLI
#[derive(Debug, Serialize, Deserialize)]
pub enum IpcMessage {
    // Volume operations
    CreateVolume {
        volume_type: VolumeType,
        size_bytes: u64,
        storage_class: String,
        name: Option<String>,
    },
    ListVolumes,
    GetVolume { id: Uuid },
    DeleteVolume { id: Uuid },
    
    // Cluster operations
    ClusterStatus,
    JoinCluster { peer_address: String },
    LeaveCluster,
    ListPeers,
    
    // Daemon operations
    Shutdown,
    Status,
    GetConfig,
    
    // Storage class operations
    CreateStorageClass { storage_class: StorageClass },
    ListStorageClasses,
}

/// IPC response types
#[derive(Debug, Serialize, Deserialize)]
pub enum IpcResponse {
    Success,
    Volume(Volume),
    Volumes(Vec<Volume>),
    StorageClasses(Vec<StorageClass>),
    ClusterInfo {
        node_id: Uuid,
        peers: Vec<String>,
        cluster_size: usize,
    },
    DaemonStatus {
        uptime: u64,
        version: String,
        storage_path: PathBuf,
        bind_address: String,
    },
    Config(DaemonConfig),
    Error(String),
}

/// The GalleonFS daemon service
pub struct GalleonDaemon {
    config: DaemonConfig,
    galleonfs: Arc<GalleonFS>,
    start_time: std::time::Instant,
}

impl GalleonDaemon {
    /// Create a new daemon instance with the given configuration
    pub fn new(config: DaemonConfig) -> Result<Self> {
        let storage_engine = Arc::new(FileStorageEngine::new(
            config.storage_path.clone(),
            config.block_size,
        ));

        let galleonfs = Arc::new(GalleonFS::new(
            storage_engine,
            config.replication_strategy.clone(),
            config.persistence_level.clone(),
            config.peer_addresses.clone(),
        ));

        Ok(Self {
            config,
            galleonfs,
            start_time: std::time::Instant::now(),
        })
    }

    /// Start the daemon service
    pub async fn start(&self) -> Result<()> {
        info!("Starting GalleonFS daemon...");
        info!("Storage path: {:?}", self.config.storage_path);
        info!("Bind address: {}", self.config.bind_address);
        info!("IPC address: {}", self.config.ipc_address);
        info!("Peers: {:?}", self.config.peer_addresses);

        // Setup default storage classes
        self.setup_default_storage_classes().await?;

        // Start the replication service
        let galleonfs_clone = Arc::clone(&self.galleonfs);
        let bind_address = self.config.bind_address.clone();
        tokio::spawn(async move {
            if let Err(e) = galleonfs_clone.run(bind_address).await {
                error!("Replication service error: {}", e);
            }
        });

        // Start mount manager if configured
        if let Some(base_mount_point) = &self.config.mount_point {
            info!("Starting mount manager at: {:?}", base_mount_point);
            std::fs::create_dir_all(base_mount_point)?;
            let _mount_manager = self.galleonfs.create_mount_manager();
        }

        // Start IPC service
        self.start_ipc_service().await?;

        Ok(())
    }

    /// Start the IPC service for CLI communication
    async fn start_ipc_service(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.config.ipc_address).await?;
        info!("IPC service listening on: {}", self.config.ipc_address);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("IPC client connected from: {}", addr);
                    let galleonfs = Arc::clone(&self.galleonfs);
                    let config = self.config.clone();
                    let start_time = self.start_time;
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_ipc_client(stream, galleonfs, config, start_time).await {
                            error!("IPC client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept IPC connection: {}", e);
                }
            }
        }
    }

    /// Handle IPC client connection
    async fn handle_ipc_client(
        mut stream: TcpStream,
        galleonfs: Arc<GalleonFS>,
        config: DaemonConfig,
        start_time: std::time::Instant,
    ) -> Result<()> {
        let mut buffer = vec![0u8; 8192];
        
        loop {
            // Read message length
            let n = stream.read(&mut buffer[..4]).await?;
            if n == 0 {
                break; // Client disconnected
            }
            
            let msg_len = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
            if msg_len > buffer.len() {
                buffer.resize(msg_len, 0);
            }
            
            // Read message data
            stream.read_exact(&mut buffer[..msg_len]).await?;
            
            // Deserialize message
            let message: IpcMessage = bincode::deserialize(&buffer[..msg_len])?;
            
            // Process message
            let response = Self::process_ipc_message(message, &galleonfs, &config, start_time).await;
            
            // Serialize response
            let response_data = bincode::serialize(&response)?;
            let response_len = (response_data.len() as u32).to_le_bytes();
            
            // Send response
            stream.write_all(&response_len).await?;
            stream.write_all(&response_data).await?;
        }
        
        Ok(())
    }

    /// Process an IPC message and return a response
    async fn process_ipc_message(
        message: IpcMessage,
        galleonfs: &Arc<GalleonFS>,
        config: &DaemonConfig,
        start_time: std::time::Instant,
    ) -> IpcResponse {
        match message {
            IpcMessage::CreateVolume { volume_type, size_bytes, storage_class, name: _ } => {
                match galleonfs.create_volume(volume_type, size_bytes, storage_class).await {
                    Ok(volume) => IpcResponse::Volume(volume),
                    Err(e) => IpcResponse::Error(format!("Failed to create volume: {}", e)),
                }
            }
            
            IpcMessage::ListVolumes => {
                match galleonfs.list_volumes().await {
                    Ok(volumes) => IpcResponse::Volumes(volumes),
                    Err(e) => IpcResponse::Error(format!("Failed to list volumes: {}", e)),
                }
            }
            
            IpcMessage::GetVolume { id } => {
                match galleonfs.get_volume(id).await {
                    Ok(Some(volume)) => IpcResponse::Volume(volume),
                    Ok(None) => IpcResponse::Error("Volume not found".to_string()),
                    Err(e) => IpcResponse::Error(format!("Failed to get volume: {}", e)),
                }
            }
            
            IpcMessage::DeleteVolume { id } => {
                match galleonfs.delete_volume(id).await {
                    Ok(_) => IpcResponse::Success,
                    Err(e) => IpcResponse::Error(format!("Failed to delete volume: {}", e)),
                }
            }
            
            IpcMessage::ClusterStatus => {
                // For now, return basic cluster info
                // TODO: Implement proper cluster management
                IpcResponse::ClusterInfo {
                    node_id: Uuid::new_v4(), // TODO: Use actual node ID
                    peers: config.peer_addresses.clone(),
                    cluster_size: config.peer_addresses.len() + 1,
                }
            }
            
            IpcMessage::ListStorageClasses => {
                match galleonfs.list_storage_classes().await {
                    Ok(classes) => IpcResponse::StorageClasses(classes),
                    Err(e) => IpcResponse::Error(format!("Failed to list storage classes: {}", e)),
                }
            }
            
            IpcMessage::Status => {
                IpcResponse::DaemonStatus {
                    uptime: start_time.elapsed().as_secs(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    storage_path: config.storage_path.clone(),
                    bind_address: config.bind_address.clone(),
                }
            }
            
            IpcMessage::GetConfig => {
                IpcResponse::Config(config.clone())
            }
            
            // TODO: Implement remaining operations
            _ => {
                IpcResponse::Error("Operation not yet implemented".to_string())
            }
        }
    }

    /// Setup default storage classes
    async fn setup_default_storage_classes(&self) -> Result<()> {
        use crate::{ReclaimPolicy, VolumeBindingMode};
        
        info!("Setting up default storage classes...");
        
        // Fast local SSD storage class
        let fast_local_ssd = StorageClass {
            name: "fast-local-ssd".to_string(),
            provisioner: "galleonfs.io/local-ssd".to_string(),
            parameters: {
                let mut params = HashMap::new();
                params.insert("disk_type".to_string(), "ssd".to_string());
                params.insert("fs_type".to_string(), "ext4".to_string());
                params.insert("encryption".to_string(), "false".to_string());
                params
            },
            reclaim_policy: ReclaimPolicy::Delete,
            volume_binding_mode: VolumeBindingMode::Immediate,
            allowed_topologies: vec!["zone-a".to_string(), "zone-b".to_string()],
            mount_options: vec!["noatime".to_string(), "nodiratime".to_string()],
        };

        // Encrypted replicated storage class
        let encrypted_storage = StorageClass {
            name: "encrypted-storage".to_string(),
            provisioner: "galleonfs.io/distributed".to_string(),
            parameters: {
                let mut params = HashMap::new();
                params.insert("encryption".to_string(), "true".to_string());
                params.insert("encryption_algorithm".to_string(), "AES-256-GCM".to_string());
                params.insert("key_management".to_string(), "internal-kms".to_string());
                params.insert("replication".to_string(), "3".to_string());
                params
            },
            reclaim_policy: ReclaimPolicy::Retain,
            volume_binding_mode: VolumeBindingMode::WaitForFirstConsumer,
            allowed_topologies: vec!["zone-a".to_string(), "zone-b".to_string(), "zone-c".to_string()],
            mount_options: vec!["noatime".to_string()],
        };

        self.galleonfs.create_storage_class(fast_local_ssd).await?;
        self.galleonfs.create_storage_class(encrypted_storage).await?;
        
        info!("Default storage classes created successfully");
        Ok(())
    }
}