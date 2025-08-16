use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use super::{Daemon, ipc::{IpcMessage, IpcReply, IpcRequest, IpcResponse}};

pub struct DaemonServer {
    daemon: Arc<Daemon>,
    listener: Option<TcpListener>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl DaemonServer {
    pub fn new(daemon: Arc<Daemon>) -> Self {
        Self {
            daemon,
            listener: None,
            shutdown_tx: None,
        }
    }

    pub async fn start(&mut self, port: u16) -> Result<()> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        info!("🚀 GalleonFS daemon server started on {}", addr);

        self.listener = Some(listener);

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let daemon_clone = self.daemon.clone();
        let listener_clone = self.listener.as_ref().unwrap();

        tokio::select! {
            result = self.accept_connections(listener_clone, daemon_clone) => {
                if let Err(e) = result {
                    error!("Server error: {}", e);
                }
            }
            _ = &mut shutdown_rx => {
                info!("🛑 Received shutdown signal");
            }
        }

        Ok(())
    }

    async fn accept_connections(&self, listener: &TcpListener, daemon: Arc<Daemon>) -> Result<()> {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("📞 New client connection from: {}", addr);
                    let daemon_clone = daemon.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, daemon_clone).await {
                            error!("Client handling error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    async fn handle_client(mut stream: TcpStream, daemon: Arc<Daemon>) -> Result<()> {
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    info!("📴 Client disconnected");
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    info!("📨 Received message: {}", trimmed);

                    let response = match IpcMessage::from_json(trimmed) {
                        Ok(message) => {
                            Self::process_request(message, &daemon).await
                        }
                        Err(e) => {
                            warn!("Failed to parse message: {}", e);
                            IpcReply::new(
                                uuid::Uuid::new_v4(),
                                IpcResponse::Error {
                                    message: format!("Invalid message format: {}", e),
                                },
                            )
                        }
                    };

                    let response_json = match response.to_json() {
                        Ok(json) => format!("{}\n", json),
                        Err(e) => {
                            error!("Failed to serialize response: {}", e);
                            format!("{{\"error\": \"Serialization error: {}\"}}\n", e)
                        }
                    };

                    if let Err(e) = writer.write_all(response_json.as_bytes()).await {
                        error!("Failed to send response: {}", e);
                        break;
                    }

                    info!("📤 Sent response: {}", response_json.trim());
                }
                Err(e) => {
                    error!("Failed to read from client: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    async fn process_request(message: IpcMessage, daemon: &Arc<Daemon>) -> IpcReply {
        info!("🔧 Processing request: {:?}", message.request);

        let response = match message.request {
            IpcRequest::CreateVolume { name, mount_path } => {
                info!("📁 Creating volume '{}' at path: {:?}", name, mount_path);
                match daemon.create_volume(name.clone(), mount_path).await {
                    Ok(volume_id) => {
                        info!("✅ Volume '{}' created successfully with ID: {}", name, volume_id);
                        IpcResponse::VolumeCreated { volume_id }
                    }
                    Err(e) => {
                        error!("❌ Failed to create volume '{}': {}", name, e);
                        IpcResponse::Error {
                            message: format!("Failed to create volume: {}", e),
                        }
                    }
                }
            }
            IpcRequest::ListVolumes => {
                info!("📋 Listing all volumes");
                let volumes = daemon.list_volumes().await;
                info!("📊 Found {} volumes", volumes.len());
                IpcResponse::VolumesList { volumes }
            }
            IpcRequest::RemoveVolume { volume_id } => {
                info!("🗑️ Removing volume with ID: {}", volume_id);
                match daemon.remove_volume(volume_id).await {
                    Ok(_) => {
                        info!("✅ Volume {} removed successfully", volume_id);
                        IpcResponse::VolumeRemoved
                    }
                    Err(e) => {
                        error!("❌ Failed to remove volume {}: {}", volume_id, e);
                        IpcResponse::Error {
                            message: format!("Failed to remove volume: {}", e),
                        }
                    }
                }
            }
            IpcRequest::GetVolumeByName { name } => {
                info!("🔍 Looking for volume with name: '{}'", name);
                let volume = daemon.get_volume_by_name(&name).await;
                if volume.is_some() {
                    info!("✅ Found volume: '{}'", name);
                } else {
                    info!("❌ Volume '{}' not found", name);
                }
                IpcResponse::Volume { volume }
            }
            IpcRequest::Ping => {
                info!("🏓 Ping received");
                IpcResponse::Pong
            }
            IpcRequest::Shutdown => {
                info!("🛑 Shutdown request received");
                IpcResponse::ShutdownComplete
            }
        };

        IpcReply::new(message.id, response)
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        info!("🛑 Shutting down daemon server...");
        
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        self.listener = None;
        info!("✅ Daemon server shutdown complete");
        Ok(())
    }
}

pub const DEFAULT_DAEMON_PORT: u16 = 8847; // GALLEON in T9