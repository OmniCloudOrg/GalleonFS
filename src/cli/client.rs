use crate::core::types::ipc::*;
use std::path::PathBuf;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct DaemonClient {
    address: String,
}

impl DaemonClient {
    pub fn new(address: String) -> Self {
        Self { address }
    }

    pub async fn send_request(&self, request: IpcRequest) -> Result<IpcResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = TcpStream::connect(&self.address).await?;
        
        let message = IpcMessage::new(request);
        let request_data = bincode::serialize(&message)?;
        
        stream.write_all(&request_data).await?;
        
        let mut buffer = vec![0u8; 4096];
        let n = stream.read(&mut buffer).await?;
        
        if n == 0 {
            return Err("No response from daemon".into());
        }
        
        let response_message: IpcResponseMessage = bincode::deserialize(&buffer[..n])?;
        Ok(response_message.response)
    }

    pub async fn create_volume(&self, name: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self.send_request(IpcRequest::CreateVolume { name: name.clone() }).await?;
        
        match response {
            IpcResponse::VolumeCreated { volume } => {
                println!("✅ Volume '{}' created successfully (ID: {})", volume.name, volume.id);
                Ok(())
            }
            IpcResponse::Error { message } => {
                eprintln!("❌ Failed to create volume '{}': {}", name, message);
                Err(message.into())
            }
            _ => Err("Unexpected response".into()),
        }
    }

    pub async fn delete_volume(&self, name: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self.send_request(IpcRequest::DeleteVolume { name: name.clone() }).await?;
        
        match response {
            IpcResponse::VolumeDeleted { name: deleted_name } => {
                println!("✅ Volume '{}' deleted successfully", deleted_name);
                Ok(())
            }
            IpcResponse::Error { message } => {
                eprintln!("❌ Failed to delete volume '{}': {}", name, message);
                Err(message.into())
            }
            _ => Err("Unexpected response".into()),
        }
    }

    pub async fn list_volumes(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self.send_request(IpcRequest::ListVolumes).await?;
        
        match response {
            IpcResponse::VolumesList { volumes } => {
                if volumes.is_empty() {
                    println!("No volumes found");
                } else {
                    println!("📋 Volumes:");
                    for volume in volumes {
                        let status = if volume.is_mounted { "🟢 mounted" } else { "🔴 unmounted" };
                        let mount_info = if let Some(ref mount_point) = volume.mount_point {
                            format!(" at '{}'", mount_point.display())
                        } else {
                            String::new()
                        };
                        
                        println!("  • {} ({}) - {}{}", 
                            volume.name, 
                            volume.id, 
                            status,
                            mount_info
                        );
                    }
                }
                Ok(())
            }
            IpcResponse::Error { message } => {
                eprintln!("❌ Failed to list volumes: {}", message);
                Err(message.into())
            }
            _ => Err("Unexpected response".into()),
        }
    }

    pub async fn mount_volume(&self, name: String, mount_point: PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self.send_request(IpcRequest::MountVolume { 
            name: name.clone(), 
            mount_point: mount_point.clone() 
        }).await?;
        
        match response {
            IpcResponse::VolumeMounted { name: mounted_name, mount_point: path } => {
                println!("✅ Volume '{}' mounted at '{}'", mounted_name, path.display());
                println!("🔍 File watcher started for this mount point");
                Ok(())
            }
            IpcResponse::Error { message } => {
                eprintln!("❌ Failed to mount volume '{}': {}", name, message);
                Err(message.into())
            }
            _ => Err("Unexpected response".into()),
        }
    }

    pub async fn unmount_volume(&self, name: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self.send_request(IpcRequest::UnmountVolume { name: name.clone() }).await?;
        
        match response {
            IpcResponse::VolumeUnmounted { name: unmounted_name } => {
                println!("✅ Volume '{}' unmounted successfully", unmounted_name);
                println!("🔍 File watcher stopped for this volume");
                Ok(())
            }
            IpcResponse::Error { message } => {
                eprintln!("❌ Failed to unmount volume '{}': {}", name, message);
                Err(message.into())
            }
            _ => Err("Unexpected response".into()),
        }
    }

    pub async fn modify_volume(&self, name: String, new_name: Option<String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self.send_request(IpcRequest::ModifyVolume { 
            name: name.clone(), 
            new_name: new_name.clone() 
        }).await?;
        
        match response {
            IpcResponse::VolumeModified { volume } => {
                if let Some(new_name) = new_name {
                    println!("✅ Volume '{}' renamed to '{}'", name, new_name);
                } else {
                    println!("✅ Volume '{}' modified successfully", volume.name);
                }
                Ok(())
            }
            IpcResponse::Error { message } => {
                eprintln!("❌ Failed to modify volume '{}': {}", name, message);
                Err(message.into())
            }
            _ => Err("Unexpected response".into()),
        }
    }
}