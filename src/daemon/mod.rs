pub mod config;
pub mod server;
pub mod watcher;

use crate::core::types::volume::Volume;
use config::{ConfigManager, DaemonConfig};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct DaemonState {
    pub volumes: Arc<RwLock<HashMap<String, Volume>>>,
    config_manager: ConfigManager,
}

impl DaemonState {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let config_manager = ConfigManager::new()?;
        let mut config = config_manager.load_config().await;
        
        // Migrate config if needed
        let migrated = config_manager.migrate_if_needed(&mut config).await?;
        if migrated {
            config_manager.save_config(&config).await?;
        }

        Ok(Self {
            volumes: Arc::new(RwLock::new(config.volumes)),
            config_manager,
        })
    }

    pub async fn save_state(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let volumes = self.volumes.read().await;
        let config = DaemonConfig {
            volumes: volumes.clone(),
            version: "0.1.0".to_string(),
        };
        
        self.config_manager.save_config(&config).await
    }

    pub async fn create_volume(&self, name: String) -> Result<Volume, String> {
        self.create_volume_with_allocation(name, 1024 * 1024 * 1024).await // Default 1GB
    }

    pub async fn create_volume_with_allocation(&self, name: String, allocation_size: u64) -> Result<Volume, String> {
        let volume = {
            let mut volumes = self.volumes.write().await;
            
            if volumes.contains_key(&name) {
                return Err(format!("Volume '{}' already exists", name));
            }

            let volume = Volume::new_with_allocation(name.clone(), allocation_size);
            volumes.insert(name, volume.clone());
            volume
        };

        // Save state to disk
        if let Err(e) = self.save_state().await {
            tracing::warn!("Failed to save state after creating volume: {}", e);
        }

        Ok(volume)
    }

    pub async fn delete_volume(&self, name: &str) -> Result<String, String> {
        let result = {
            let mut volumes = self.volumes.write().await;
            
            match volumes.remove(name) {
                Some(volume) => {
                    if volume.is_mounted {
                        volumes.insert(name.to_string(), volume); // Restore the volume
                        return Err(format!("Volume '{}' is currently mounted. Unmount it first.", name));
                    }
                    Ok(name.to_string())
                }
                None => Err(format!("Volume '{}' not found", name)),
            }
        };

        // Save state to disk if deletion was successful
        if result.is_ok() {
            if let Err(e) = self.save_state().await {
                tracing::warn!("Failed to save state after deleting volume: {}", e);
            }
        }

        result
    }

    pub async fn list_volumes(&self) -> Vec<Volume> {
        let volumes = self.volumes.read().await;
        volumes.values().cloned().collect()
    }

    pub async fn mount_volume(&self, name: &str, mount_point: PathBuf) -> Result<(String, PathBuf), String> {
        let result = {
            let mut volumes = self.volumes.write().await;
            
            match volumes.get_mut(name) {
                Some(volume) => {
                    if volume.is_mounted {
                        return Err(format!("Volume '{}' is already mounted", name));
                    }
                    
                    volume.mount(mount_point.clone());
                    Ok((name.to_string(), mount_point))
                }
                None => Err(format!("Volume '{}' not found", name)),
            }
        };

        // Save state to disk if mount was successful
        if result.is_ok() {
            if let Err(e) = self.save_state().await {
                tracing::warn!("Failed to save state after mounting volume: {}", e);
            }
        }

        result
    }

    pub async fn unmount_volume(&self, name: &str) -> Result<String, String> {
        let result = {
            let mut volumes = self.volumes.write().await;
            
            match volumes.get_mut(name) {
                Some(volume) => {
                    if !volume.is_mounted {
                        return Err(format!("Volume '{}' is not mounted", name));
                    }
                    
                    volume.unmount();
                    Ok(name.to_string())
                }
                None => Err(format!("Volume '{}' not found", name)),
            }
        };

        // Save state to disk if unmount was successful
        if result.is_ok() {
            if let Err(e) = self.save_state().await {
                tracing::warn!("Failed to save state after unmounting volume: {}", e);
            }
        }

        result
    }

    pub async fn modify_volume(&self, name: &str, new_name: Option<String>) -> Result<Volume, String> {
        let result = {
            let mut volumes = self.volumes.write().await;
            
            if let Some(new_name) = new_name {
                if volumes.contains_key(&new_name) && &new_name != name {
                    return Err(format!("Volume '{}' already exists", new_name));
                }
                
                if let Some(mut volume) = volumes.remove(name) {
                    volume.name = new_name.clone();
                    volumes.insert(new_name, volume.clone());
                    Ok(volume)
                } else {
                    Err(format!("Volume '{}' not found", name))
                }
            } else {
                volumes.get(name).cloned().ok_or_else(|| format!("Volume '{}' not found", name))
            }
        };

        // Save state to disk if modification was successful
        if result.is_ok() {
            if let Err(e) = self.save_state().await {
                tracing::warn!("Failed to save state after modifying volume: {}", e);
            }
        }

        result
    }

    pub async fn get_mounted_volumes(&self) -> Vec<Volume> {
        let volumes = self.volumes.read().await;
        volumes.values()
            .filter(|v| v.is_mounted)
            .cloned()
            .collect()
    }
}