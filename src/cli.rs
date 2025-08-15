//! GalleonFS CLI Module
//!
//! This module provides the command-line interface for GalleonFS, which
//! communicates with the daemon to perform various operations.

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::io::{self, BufRead};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::error;
use uuid::Uuid;

use crate::{VolumeType, daemon::{IpcMessage, IpcResponse}};

/// CLI arguments and subcommands
#[derive(Parser)]
#[command(name = "galleonfs")]
#[command(about = "A distributed, high-performance, network-replicated filesystem")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    /// Address of the daemon IPC service
    #[arg(long, default_value = "127.0.0.1:8090")]
    pub daemon_address: String,
    
    /// Subcommands
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Volume management commands
    Volume {
        #[command(subcommand)]
        action: VolumeCommands,
    },
    
    /// Cluster management commands
    Cluster {
        #[command(subcommand)]
        action: ClusterCommands,
    },
    
    /// Daemon management commands
    Daemon {
        #[command(subcommand)]
        action: DaemonCommands,
    },
    
    /// Storage class management commands
    StorageClass {
        #[command(subcommand)]
        action: StorageClassCommands,
    },
}

#[derive(Subcommand)]
pub enum VolumeCommands {
    /// Create a new volume
    Create {
        /// Name for the volume
        #[arg(short, long)]
        name: Option<String>,
        
        /// Volume type
        #[arg(short, long, default_value = "persistent")]
        volume_type: CliVolumeType,
        
        /// Size in bytes (supports suffixes: K, M, G, T)
        #[arg(short, long, default_value = "1G")]
        size: String,
        
        /// Storage class to use
        #[arg(short = 'c', long, default_value = "encrypted-storage")]
        storage_class: String,
        
        /// Number of replicas (overrides storage class setting)
        #[arg(short, long)]
        replicas: Option<u32>,
    },
    
    /// List all volumes
    List,
    
    /// Get details of a specific volume
    Get {
        /// Volume ID or name
        id: String,
    },
    
    /// Delete a volume
    Delete {
        /// Volume ID or name
        id: String,
        
        /// Force deletion without confirmation
        #[arg(short, long)]
        force: bool,
    },
    
    /// Mount a volume
    Mount {
        /// Volume ID or name
        id: String,
        
        /// Mount point path
        path: PathBuf,
        
        /// Mount options
        #[arg(short, long, value_delimiter = ',')]
        options: Vec<String>,
    },
    
    /// Unmount a volume
    Unmount {
        /// Volume ID or mount path
        target: String,
    },
}

#[derive(Subcommand)]
pub enum ClusterCommands {
    /// Show cluster status
    Status,
    
    /// Join an existing cluster
    Join {
        /// Address of a node in the cluster to join
        peer_address: String,
    },
    
    /// Leave the current cluster
    Leave {
        /// Force leave without graceful handoff
        #[arg(short, long)]
        force: bool,
    },
    
    /// List all peers in the cluster
    Peers,
    
    /// Add a replication peer
    AddPeer {
        /// Address of the peer to add for replication
        peer_address: String,
    },
    
    /// Remove a replication peer
    RemovePeer {
        /// Address of the peer to remove from replication
        peer_address: String,
    },
    
    /// List current replication peers
    ReplicationPeers,
}

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// Show daemon status
    Status,
    
    /// Show daemon configuration
    Config,
    
    /// Stop the daemon
    Stop {
        /// Force stop without graceful shutdown
        #[arg(short, long)]
        force: bool,
    },
    
    /// Restart the daemon
    Restart,
}

#[derive(Subcommand)]
pub enum StorageClassCommands {
    /// List all storage classes
    List,
    
    /// Create a new storage class
    Create {
        /// Storage class configuration file (JSON/YAML)
        config_file: PathBuf,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum CliVolumeType {
    Ephemeral,
    Persistent,
    Shared,
}

impl From<CliVolumeType> for VolumeType {
    fn from(cli_type: CliVolumeType) -> Self {
        match cli_type {
            CliVolumeType::Ephemeral => VolumeType::Ephemeral,
            CliVolumeType::Persistent => VolumeType::Persistent,
            CliVolumeType::Shared => VolumeType::Shared,
        }
    }
}

/// CLI client for communicating with the daemon
pub struct GalleonClient {
    daemon_address: String,
}

impl GalleonClient {
    /// Create a new CLI client
    pub fn new(daemon_address: String) -> Self {
        Self { daemon_address }
    }

    /// Execute a CLI command
    pub async fn execute(&self, cli: Cli) -> Result<()> {
        match cli.command {
            Commands::Volume { action } => {
                self.handle_volume_command(action).await
            }
            Commands::Cluster { action } => {
                self.handle_cluster_command(action).await
            }
            Commands::Daemon { action } => {
                self.handle_daemon_command(action).await
            }
            Commands::StorageClass { action } => {
                self.handle_storage_class_command(action).await
            }
        }
    }

    /// Handle volume management commands
    async fn handle_volume_command(&self, action: VolumeCommands) -> Result<()> {
        match action {
            VolumeCommands::Create { name, volume_type, size, storage_class, replicas } => {
                let size_bytes = Self::parse_size(&size)?;
                
                // TODO: Handle replicas parameter
                if replicas.is_some() {
                    println!("Note: --replicas parameter will override storage class replication settings");
                }
                
                let message = IpcMessage::CreateVolume {
                    volume_type: volume_type.into(),
                    size_bytes,
                    storage_class,
                    name,
                };
                
                match self.send_message(message).await? {
                    IpcResponse::Volume(volume) => {
                        println!("Volume created successfully:");
                        println!("  ID: {}", volume.id);
                        println!("  Name: {}", volume.name);
                        println!("  Type: {:?}", volume.volume_type);
                        println!("  Size: {} bytes ({:.2} GB)", volume.size_bytes, volume.size_bytes as f64 / 1_073_741_824.0);
                        println!("  Storage Class: {}", volume.storage_class);
                        println!("  State: {:?}", volume.state);
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to create volume: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            VolumeCommands::List => {
                let message = IpcMessage::ListVolumes;
                
                match self.send_message(message).await? {
                    IpcResponse::Volumes(volumes) => {
                        if volumes.is_empty() {
                            println!("No volumes found.");
                        } else {
                            println!("Volumes:");
                            println!("{:<36} {:<20} {:<12} {:<12} {:<20} {:<10}", 
                                    "ID", "NAME", "TYPE", "STATE", "STORAGE_CLASS", "SIZE");
                            println!("{}", "-".repeat(112));
                            
                            for volume in volumes {
                                let size_str = if volume.size_bytes >= 1_073_741_824 {
                                    format!("{:.1}GB", volume.size_bytes as f64 / 1_073_741_824.0)
                                } else if volume.size_bytes >= 1_048_576 {
                                    format!("{:.1}MB", volume.size_bytes as f64 / 1_048_576.0)
                                } else if volume.size_bytes >= 1024 {
                                    format!("{:.1}KB", volume.size_bytes as f64 / 1024.0)
                                } else {
                                    format!("{}B", volume.size_bytes)
                                };
                                
                                println!("{:<36} {:<20} {:<12?} {:<12?} {:<20} {:<10}", 
                                        volume.id, volume.name, volume.volume_type, 
                                        volume.state, volume.storage_class, size_str);
                            }
                        }
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to list volumes: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            VolumeCommands::Get { id } => {
                // Try to parse as UUID, otherwise treat as name
                let message = if let Ok(uuid) = Uuid::parse_str(&id) {
                    IpcMessage::GetVolume { id: uuid }
                } else {
                    IpcMessage::GetVolumeByName { name: id.clone() }
                };
                
                match self.send_message(message).await? {
                    IpcResponse::Volume(volume) => {
                        println!("Volume Details:");
                        println!("  ID: {}", volume.id);
                        println!("  Name: {}", volume.name);
                        println!("  Type: {:?}", volume.volume_type);
                        println!("  Size: {} bytes ({:.2} GB)", volume.size_bytes, volume.size_bytes as f64 / 1_073_741_824.0);
                        println!("  Storage Class: {}", volume.storage_class);
                        println!("  State: {:?}", volume.state);
                        println!("  Created: {:?}", volume.created_at);
                        println!("  Updated: {:?}", volume.updated_at);
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to get volume: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            VolumeCommands::Delete { id, force } => {
                if !force {
                    println!("Are you sure you want to delete volume '{}'? [y/N]", id);
                    let mut input = String::new();
                    let mut stdin = io::BufReader::new(io::stdin());
                    stdin.read_line(&mut input)?;
                    if !input.trim().to_lowercase().starts_with('y') {
                        println!("Volume deletion cancelled.");
                        return Ok(());
                    }
                }
                
                // Try to parse as UUID, otherwise treat as name
                let message = if let Ok(uuid) = Uuid::parse_str(&id) {
                    IpcMessage::DeleteVolume { id: uuid }
                } else {
                    IpcMessage::DeleteVolumeByName { name: id.clone() }
                };
                
                match self.send_message(message).await? {
                    IpcResponse::Success => {
                        println!("Volume '{}' deleted successfully.", id);
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to delete volume: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            // Mount/unmount operations
            VolumeCommands::Mount { id, path, options } => {
                // Try to parse as UUID, otherwise get volume by name first
                let volume_id = if let Ok(uuid) = Uuid::parse_str(&id) {
                    uuid
                } else {
                    // Get volume by name to get its ID
                    let name_message = IpcMessage::GetVolumeByName { name: id.clone() };
                    match self.send_message(name_message).await? {
                        IpcResponse::Volume(volume) => volume.id,
                        IpcResponse::Error(err) => {
                            error!("Failed to find volume '{}': {}", id, err);
                            return Err(anyhow::anyhow!(err));
                        }
                        _ => {
                            return Err(anyhow::anyhow!("Unexpected response from daemon"));
                        }
                    }
                };
                
                let message = IpcMessage::MountVolume { 
                    id: volume_id, 
                    path: path.clone(), 
                    options 
                };
                
                match self.send_message(message).await? {
                    IpcResponse::MountInfo { mount_id, volume_id, mount_path } => {
                        println!("Volume mounted successfully:");
                        println!("  Mount ID: {}", mount_id);
                        println!("  Volume ID: {}", volume_id);
                        println!("  Mount Path: {}", mount_path.display());
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to mount volume: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            VolumeCommands::Unmount { target } => {
                let message = IpcMessage::UnmountVolume { target: target.clone() };
                
                match self.send_message(message).await? {
                    IpcResponse::Success => {
                        println!("Volume unmounted successfully: {}", target);
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to unmount volume: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Handle cluster management commands
    async fn handle_cluster_command(&self, action: ClusterCommands) -> Result<()> {
        match action {
            ClusterCommands::Status => {
                let message = IpcMessage::ClusterStatus;
                
                match self.send_message(message).await? {
                    IpcResponse::ClusterInfo { node_id, peers, cluster_size } => {
                        println!("Cluster Status:");
                        println!("  Node ID: {}", node_id);
                        println!("  Cluster Size: {} nodes", cluster_size);
                        println!("  Peers:");
                        if peers.is_empty() {
                            println!("    (No peers configured)");
                        } else {
                            for peer in peers {
                                println!("    - {}", peer);
                            }
                        }
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to get cluster status: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            ClusterCommands::Join { peer_address } => {
                let message = IpcMessage::JoinCluster { peer_address: peer_address.clone() };
                
                match self.send_message(message).await? {
                    IpcResponse::Success => {
                        println!("Successfully joined cluster via peer: {}", peer_address);
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to join cluster: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            ClusterCommands::Leave { force } => {
                let message = IpcMessage::LeaveCluster { force };
                
                match self.send_message(message).await? {
                    IpcResponse::Success => {
                        println!("Successfully left cluster");
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to leave cluster: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            ClusterCommands::Peers => {
                // Show the same information as cluster status but focus on peers
                let message = IpcMessage::ClusterStatus;
                
                match self.send_message(message).await? {
                    IpcResponse::ClusterInfo { node_id: _, peers, cluster_size: _ } => {
                        println!("Cluster Peers:");
                        if peers.is_empty() {
                            println!("  (No peers configured)");
                        } else {
                            for peer in peers {
                                println!("  - {}", peer);
                            }
                        }
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to get cluster peers: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            ClusterCommands::AddPeer { peer_address } => {
                let message = IpcMessage::AddReplicationPeer { peer_address: peer_address.clone() };
                
                match self.send_message(message).await? {
                    IpcResponse::Success => {
                        println!("Successfully added replication peer: {}", peer_address);
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to add replication peer: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            ClusterCommands::RemovePeer { peer_address } => {
                let message = IpcMessage::RemoveReplicationPeer { peer_address: peer_address.clone() };
                
                match self.send_message(message).await? {
                    IpcResponse::Success => {
                        println!("Successfully removed replication peer: {}", peer_address);
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to remove replication peer: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            ClusterCommands::ReplicationPeers => {
                let message = IpcMessage::ListReplicationPeers;
                
                match self.send_message(message).await? {
                    IpcResponse::ReplicationPeers(peers) => {
                        println!("Replication Peers:");
                        if peers.is_empty() {
                            println!("  (No replication peers configured)");
                        } else {
                            for peer in peers {
                                println!("  - {}", peer);
                            }
                        }
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to get replication peers: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Handle daemon management commands
    async fn handle_daemon_command(&self, action: DaemonCommands) -> Result<()> {
        match action {
            DaemonCommands::Status => {
                let message = IpcMessage::Status;
                
                match self.send_message(message).await? {
                    IpcResponse::DaemonStatus { uptime, version, storage_path, bind_address } => {
                        println!("Daemon Status:");
                        println!("  Version: {}", version);
                        println!("  Uptime: {} seconds", uptime);
                        println!("  Storage Path: {:?}", storage_path);
                        println!("  Bind Address: {}", bind_address);
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to get daemon status: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            DaemonCommands::Config => {
                let message = IpcMessage::GetConfig;
                
                match self.send_message(message).await? {
                    IpcResponse::Config(config) => {
                        println!("Daemon Configuration:");
                        println!("  Storage Path: {:?}", config.storage_path);
                        println!("  Block Size: {} bytes", config.block_size);
                        println!("  Bind Address: {}", config.bind_address);
                        println!("  IPC Address: {}", config.ipc_address);
                        println!("  Replication Strategy: {:?}", config.replication_strategy);
                        println!("  Persistence Level: {:?}", config.persistence_level);
                        println!("  Peer Addresses: {:?}", config.peer_addresses);
                        if let Some(mount_point) = &config.mount_point {
                            println!("  Mount Point: {:?}", mount_point);
                        }
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to get daemon config: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            DaemonCommands::Stop { force: _ } => {
                let message = IpcMessage::Shutdown;
                
                match self.send_message(message).await? {
                    IpcResponse::Success => {
                        println!("Daemon shutdown requested.");
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to stop daemon: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            DaemonCommands::Restart => {
                let message = IpcMessage::Restart;
                
                match self.send_message(message).await? {
                    IpcResponse::Success => {
                        println!("Daemon restart initiated.");
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to restart daemon: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Handle storage class management commands
    async fn handle_storage_class_command(&self, action: StorageClassCommands) -> Result<()> {
        match action {
            StorageClassCommands::List => {
                let message = IpcMessage::ListStorageClasses;
                
                match self.send_message(message).await? {
                    IpcResponse::StorageClasses(classes) => {
                        if classes.is_empty() {
                            println!("No storage classes found.");
                        } else {
                            println!("Storage Classes:");
                            println!("{:<25} {:<25} {:<15} {:<20}", "NAME", "PROVISIONER", "RECLAIM_POLICY", "BINDING_MODE");
                            println!("{}", "-".repeat(85));
                            
                            for class in classes {
                                println!("{:<25} {:<25} {:<15?} {:<20?}", 
                                        class.name, class.provisioner, class.reclaim_policy, class.volume_binding_mode);
                            }
                        }
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to list storage classes: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
            
            StorageClassCommands::Create { config_file } => {
                let message = IpcMessage::CreateStorageClassFromFile { config_file: config_file.clone() };
                
                match self.send_message(message).await? {
                    IpcResponse::Success => {
                        println!("Storage class created successfully from file: {}", config_file.display());
                    }
                    IpcResponse::Error(err) => {
                        error!("Failed to create storage class: {}", err);
                        return Err(anyhow::anyhow!(err));
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unexpected response from daemon"));
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Send a message to the daemon and receive the response
    async fn send_message(&self, message: IpcMessage) -> Result<IpcResponse> {
        let mut stream = TcpStream::connect(&self.daemon_address).await
            .map_err(|e| anyhow::anyhow!("Failed to connect to daemon at {}: {}. Is the daemon running?", self.daemon_address, e))?;
        
        // Serialize message
        let message_data = bincode::serialize(&message)?;
        let message_len = (message_data.len() as u32).to_le_bytes();
        
        // Send message
        stream.write_all(&message_len).await?;
        stream.write_all(&message_data).await?;
        
        // Read response length
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let response_len = u32::from_le_bytes(len_buf) as usize;
        
        // Read response data
        let mut response_buf = vec![0u8; response_len];
        stream.read_exact(&mut response_buf).await?;
        
        // Deserialize response
        let response: IpcResponse = bincode::deserialize(&response_buf)?;
        
        Ok(response)
    }

    /// Parse size string (e.g., "1G", "512M", "2048K") into bytes
    fn parse_size(size_str: &str) -> Result<u64> {
        let size_str = size_str.trim().to_uppercase();
        
        if let Some(stripped) = size_str.strip_suffix('T') {
            Ok(stripped.parse::<u64>()? * 1_099_511_627_776) // 1024^4
        } else if let Some(stripped) = size_str.strip_suffix('G') {
            Ok(stripped.parse::<u64>()? * 1_073_741_824) // 1024^3
        } else if let Some(stripped) = size_str.strip_suffix('M') {
            Ok(stripped.parse::<u64>()? * 1_048_576) // 1024^2
        } else if let Some(stripped) = size_str.strip_suffix('K') {
            Ok(stripped.parse::<u64>()? * 1024)
        } else if let Some(stripped) = size_str.strip_suffix('B') {
            Ok(stripped.parse::<u64>()?)
        } else {
            // No suffix, assume bytes
            Ok(size_str.parse::<u64>()?)
        }
    }
}