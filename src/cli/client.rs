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

    pub async fn create_volume(&self, name: String, size: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let allocation_size = self.parse_size(&size)?;
        let response = self.send_request(IpcRequest::CreateVolume { 
            name: name.clone(),
            allocation_size 
        }).await?;
        
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
        let usage_width = 15; // For "current/allocated" format like "1.2GB/5.0GB"
        let mount_width = cmp::max(12, volumes.iter().map(|v| {
            if let Some(ref mp) = v.mount_point {
                mp.display().to_string().len()
            } else {
                1 // for "-"
            }
        }).max().unwrap_or(0));

        // Print header with bold blue formatting
        println!("\x1b[1;34m{:<name_width$}  {:<uuid_width$}  {:<status_width$}  {:<usage_width$}  {:<mount_width$}\x1b[0m", 
                "Name", "UUID", "Status", "Usage", "Mount Point(s)",
                name_width = name_width,
                uuid_width = uuid_width,
                status_width = status_width,
                usage_width = usage_width,
                mount_width = mount_width);
        
        // Print separator
        println!("{}  {}  {}  {}  {}", 
                "-".repeat(name_width),
                "-".repeat(uuid_width), 
                "-".repeat(status_width),
                "-".repeat(usage_width),
                "-".repeat(mount_width));

        // Print volume rows
        for volume in volumes {
            let status_colored = if volume.is_mounted {
                "\x1b[32mmounted\x1b[0m"  // Green
            } else {
                "\x1b[31munmounted\x1b[0m"  // Red
            };

            // Format usage as current/allocated with color coding
            let usage = self.format_usage_with_color(volume.current_size, volume.allocated_size);

            // Collect mount points (currently single, but structured for future multi-mount support)
            let mount_points: Vec<String> = if let Some(ref mp) = volume.mount_point {
                vec![mp.display().to_string()]
            } else {
                vec!["-".to_string()]
            };

            // Print first row with all columns
            print!("{:<name_width$}  {:<uuid_width$}  ", 
                   volume.name, volume.id,
                   name_width = name_width,
                   uuid_width = uuid_width);
            
            // Print colored status with manual padding
            print!("{}", status_colored);
            let status_text_len = if volume.is_mounted { 7 } else { 9 }; // "mounted" vs "unmounted"
            print!("{}  ", " ".repeat(status_width - status_text_len));
            
            // Print usage and mount point - usage may contain color codes, so handle spacing manually
            print!("{}  ", usage);
            let usage_text_len = self.get_text_length_without_colors(&usage);
            print!("{}", " ".repeat(usage_width.saturating_sub(usage_text_len)));
            println!("{}", mount_points[0]);

            // Print additional mount points (if any) with proper spacing
            for mount_point in mount_points.iter().skip(1) {
                println!("{:<name_width$}  {:<uuid_width$}  {:<status_width$}  {:<usage_width$}  {}", 
                        "", "", "", "", mount_point,
                        name_width = name_width,
                        uuid_width = uuid_width,
                        status_width = status_width,
                        usage_width = usage_width);
            }
        }
    }

    fn format_bytes_ratio(&self, current: u64, allocated: u64) -> String {
        fn format_bytes(bytes: u64) -> String {
            const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
            let mut size = bytes as f64;
            let mut unit_index = 0;
            
            while size >= 1024.0 && unit_index < UNITS.len() - 1 {
                size /= 1024.0;
                unit_index += 1;
            }
            
            if unit_index == 0 {
                format!("{}B", bytes)
            } else {
                format!("{:.1}{}", size, UNITS[unit_index])
            }
        }
        
        format!("{}/{}", format_bytes(current), format_bytes(allocated))
    }

    fn format_usage_with_color(&self, current: u64, allocated: u64) -> String {
        fn format_bytes(bytes: u64) -> String {
            const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
            let mut size = bytes as f64;
            let mut unit_index = 0;
            
            while size >= 1024.0 && unit_index < UNITS.len() - 1 {
                size /= 1024.0;
                unit_index += 1;
            }
            
            if unit_index == 0 {
                format!("{}B", bytes)
            } else {
                format!("{:.1}{}", size, UNITS[unit_index])
            }
        }
        
        let usage_text = format!("{}/{}", format_bytes(current), format_bytes(allocated));
        let usage_percent = if allocated > 0 {
            (current as f64 / allocated as f64) * 100.0
        } else if current > 0 {
            100.0  // If no allocation but has usage, treat as 100% (over-allocated)
        } else {
            0.0   // Both are 0
        };
        
        // Color coding based on usage percentage
        // <75% white, 75-85% yellow, 85-90% orange, 90-100% red, 100% white text on red background
        if usage_percent >= 100.0 {
            format!("\x1b[37;41m{}\x1b[0m", usage_text)  // White text on red background
        } else if usage_percent >= 90.0 {
            format!("\x1b[31m{}\x1b[0m", usage_text)     // Red
        } else if usage_percent >= 85.0 {
            format!("\x1b[91m{}\x1b[0m", usage_text)     // Orange (bright red)
        } else if usage_percent >= 75.0 {
            format!("\x1b[33m{}\x1b[0m", usage_text)     // Yellow
        } else {
            usage_text  // White (default)
        }
    }

    fn get_text_length_without_colors(&self, text: &str) -> usize {
        let mut length = 0;
        let mut chars = text.chars();
        
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // Skip ANSI escape sequence
                while let Some(escape_ch) = chars.next() {
                    if escape_ch.is_alphabetic() {
                        break;
                    }
                }
            } else {
                length += 1;
            }
        }
        
        length
    }

    fn parse_size(&self, size_str: &str) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let size_str = size_str.trim().to_uppercase();
        
        if size_str.is_empty() {
            return Err("Empty size string".into());
        }
        
        // Extract numeric part and unit
        let (number_str, unit) = if let Some(last_char) = size_str.chars().last() {
            if last_char.is_alphabetic() {
                let mut chars = size_str.chars();
                let unit_char = chars.next_back().unwrap();
                let number_part: String = chars.collect();
                (number_part, unit_char.to_string())
            } else {
                (size_str, "B".to_string())
            }
        } else {
            return Err("Invalid size format".into());
        };
        
        // Parse the numeric part
        let number: f64 = number_str.parse()
            .map_err(|_| format!("Invalid number: {}", number_str))?;
        
        if number < 0.0 {
            return Err("Size cannot be negative".into());
        }
        
        // Convert based on unit
        let multiplier = match unit.as_str() {
            "B" => 1,
            "K" => 1024,
            "M" => 1024 * 1024,
            "G" => 1024 * 1024 * 1024,
            "T" => 1024_u64.pow(4),
            "P" => 1024_u64.pow(5),
            "E" => 1024_u64.pow(6),
            "Z" => 1024_u64.pow(7),
            "Y" => 1024_u64.pow(8),
            _ => return Err(format!("Unknown unit: {}. Supported: B, K, M, G, T, P, E, Z, Y", unit).into()),
        };
        
        let bytes = (number * multiplier as f64) as u64;
        Ok(bytes)
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

    pub async fn modify_volume(&self, name: String, new_name: Option<String>, size: Option<String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let new_allocation_size = if let Some(size_str) = size {
            Some(self.parse_size(&size_str)?)
        } else {
            None
        };

        let response = self.send_request(IpcRequest::ModifyVolume { 
            name: name.clone(), 
            new_name: new_name.clone(),
            new_allocation_size
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

    pub async fn set_volume_usage(&self, name: String, usage: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let usage_size = self.parse_size(&usage)?;
        let response = self.send_request(IpcRequest::SetUsage { 
            name: name.clone(), 
            usage_size 
        }).await?;
        
        match response {
            IpcResponse::VolumeModified { volume } => {
                println!("✅ Volume '{}' usage set to {} ({}%)", 
                    volume.name, 
                    usage,
                    if volume.allocated_size > 0 {
                        format!("{:.1}", (volume.current_size as f64 / volume.allocated_size as f64) * 100.0)
                    } else {
                        "N/A".to_string()
                    });
                Ok(())
            }
            IpcResponse::Error { message } => {
                eprintln!("❌ Failed to set usage for volume '{}': {}", name, message);
                Err(message.into())
            }
            _ => Err("Unexpected response".into()),
        }
    }
}