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
                    self.print_volumes_table(&volumes);
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

    fn print_volumes_table(&self, volumes: &[crate::core::types::volume::Volume]) {
        use std::cmp;
        
        // Calculate column widths
        let name_width = cmp::max(4, volumes.iter().map(|v| v.name.len()).max().unwrap_or(0));
        let uuid_width = 36; // UUIDs are always 36 chars
        let status_width = 9; // "mounted" or "unmounted" (without color codes)
        let mount_width = cmp::max(12, volumes.iter().map(|v| {
            if let Some(ref mp) = v.mount_point {
                mp.display().to_string().len()
            } else {
                1 // for "-"
            }
        }).max().unwrap_or(0));

        // Print header with bold formatting
        println!("\x1b[1m{:<name_width$} {:<uuid_width$} {:<status_width$} {:<mount_width$}\x1b[0m", 
                "Name", "UUID", "Status", "Mount Point(s)",
                name_width = name_width,
                uuid_width = uuid_width,
                status_width = status_width,
                mount_width = mount_width);
        
        // Print separator
        println!("{} {} {} {}", 
                "-".repeat(name_width),
                "-".repeat(uuid_width), 
                "-".repeat(status_width),
                "-".repeat(mount_width));

        // Print volume rows
        for volume in volumes {
            let status_colored = if volume.is_mounted {
                "\x1b[32mmounted\x1b[0m"  // Green
            } else {
                "\x1b[31munmounted\x1b[0m"  // Red
            };

            // Collect mount points (currently single, but structured for future multi-mount support)
            let mount_points: Vec<String> = if let Some(ref mp) = volume.mount_point {
                vec![mp.display().to_string()]
            } else {
                vec!["-".to_string()]
            };

            // Print first row with all columns
            print!("{:<name_width$} {:<uuid_width$} ", 
                   volume.name, volume.id,
                   name_width = name_width,
                   uuid_width = uuid_width);
            
            // Print colored status with manual padding
            print!("{}", status_colored);
            let status_text_len = if volume.is_mounted { 7 } else { 9 }; // "mounted" vs "unmounted"
            print!("{}", " ".repeat(status_width - status_text_len + 1));
            
            // Print first mount point
            println!("{}", mount_points[0]);

            // Print additional mount points (if any) with proper spacing
            for mount_point in mount_points.iter().skip(1) {
                println!("{:<name_width$} {:<uuid_width$} {:<status_width$} {}", 
                        "", "", "", mount_point,
                        name_width = name_width,
                        uuid_width = uuid_width,
                        status_width = status_width);
            }
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