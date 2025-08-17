use crate::core::types::ipc::*;
use crate::daemon::{DaemonState, watcher::FileWatcher};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tracing::{info, error, warn, debug};

pub struct DaemonServer {
    state: Arc<DaemonState>,
    watcher: Arc<Mutex<FileWatcher>>,
}

impl DaemonServer {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let state = Arc::new(DaemonState::new().await?);
        Ok(Self {
            state,
            watcher: Arc::new(Mutex::new(FileWatcher::new())),
        })
    }

    pub async fn start(&self, address: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(address).await?;
        info!("GalleonFS daemon listening on {}", address);

        let mounted_volumes = self.state.get_mounted_volumes().await;
        self.watcher.lock().await.start_watching(mounted_volumes).await;

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("New client connected: {}", addr);
                    let state = Arc::clone(&self.state);
                    let watcher = Arc::clone(&self.watcher);
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, state, watcher).await {
                            error!("Error handling client {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept client connection: {}", e);
                }
            }
        }
    }

    async fn handle_client(
        mut stream: TcpStream,
        state: Arc<DaemonState>,
        watcher: Arc<Mutex<FileWatcher>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut buffer = vec![0u8; 4096];
        
        loop {
            let n = stream.read(&mut buffer).await?;
            if n == 0 {
                debug!("Client disconnected");
                break;
            }

            let message_data = &buffer[..n];
            
            match bincode::deserialize::<IpcMessage>(message_data) {
                Ok(message) => {
                    let response = Self::process_request(&message.request, &state, &watcher).await;
                    let response_message = IpcResponseMessage::new(message.id, response);
                    
                    match bincode::serialize(&response_message) {
                        Ok(response_data) => {
                            if let Err(e) = stream.write_all(&response_data).await {
                                error!("Failed to send response: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Failed to serialize response: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to deserialize request: {}", e);
                    let error_response = IpcResponse::Error {
                        message: format!("Invalid request format: {}", e),
                    };
                    let response_message = IpcResponseMessage::new(uuid::Uuid::new_v4(), error_response);
                    
                    if let Ok(response_data) = bincode::serialize(&response_message) {
                        let _ = stream.write_all(&response_data).await;
                    }
                }
            }
        }

        Ok(())
    }

    async fn process_request(
        request: &IpcRequest,
        state: &Arc<DaemonState>,
        watcher: &Arc<Mutex<FileWatcher>>,
    ) -> IpcResponse {
        match request {
            IpcRequest::CreateVolume { name, allocation_size } => {
                match state.create_volume_with_allocation(name.clone(), *allocation_size).await {
                    Ok(volume) => IpcResponse::VolumeCreated { volume },
                    Err(error) => IpcResponse::Error { message: error },
                }
            }
            
            IpcRequest::DeleteVolume { name } => {
                watcher.lock().await.remove_volume(name).await;
                match state.delete_volume(name).await {
                    Ok(deleted_name) => IpcResponse::VolumeDeleted { name: deleted_name },
                    Err(error) => IpcResponse::Error { message: error },
                }
            }
            
            IpcRequest::ListVolumes => {
                let volumes = state.list_volumes().await;
                IpcResponse::VolumesList { volumes }
            }
            
            IpcRequest::MountVolume { name, mount_point } => {
                match state.mount_volume(name, mount_point.clone()).await {
                    Ok((volume_name, path)) => {
                        let volumes = state.get_mounted_volumes().await;
                        if let Some(volume) = volumes.iter().find(|v| v.name == *name) {
                            watcher.lock().await.add_volume(volume).await;
                        }
                        IpcResponse::VolumeMounted { 
                            name: volume_name, 
                            mount_point: path 
                        }
                    }
                    Err(error) => IpcResponse::Error { message: error },
                }
            }
            
            IpcRequest::UnmountVolume { name } => {
                watcher.lock().await.remove_volume(name).await;
                match state.unmount_volume(name).await {
                    Ok(volume_name) => IpcResponse::VolumeUnmounted { name: volume_name },
                    Err(error) => IpcResponse::Error { message: error },
                }
            }
            
            IpcRequest::ModifyVolume { name, new_name, new_allocation_size } => {
                match state.modify_volume(name, new_name.clone(), *new_allocation_size).await {
                    Ok(volume) => IpcResponse::VolumeModified { volume },
                    Err(error) => IpcResponse::Error { message: error },
                }
            }
            
            IpcRequest::Shutdown => {
                info!("Shutdown request received, saving state...");
                if let Err(e) = state.save_state().await {
                    error!("Failed to save state during shutdown: {}", e);
                } else {
                    info!("State saved successfully");
                }
                std::process::exit(0);
            }
        }
    }
}