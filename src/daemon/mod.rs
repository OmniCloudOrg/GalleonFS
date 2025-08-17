pub mod server;
pub mod watcher;

use crate::core::types::volume::Volume;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct DaemonState {
    pub volumes: Arc<RwLock<HashMap<String, Volume>>>,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            volumes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_volume(&self, name: String) -> Result<Volume, String> {
        let mut volumes = self.volumes.write().await;
        
        if volumes.contains_key(&name) {
            return Err(format!("Volume '{}' already exists", name));
        }

        let volume = Volume::new(name.clone());
        volumes.insert(name, volume.clone());
        Ok(volume)
    }

    pub async fn delete_volume(&self, name: &str) -> Result<String, String> {
        let mut volumes = self.volumes.write().await;
        
        match volumes.remove(name) {
            Some(volume) => {
                if volume.is_mounted {
                    return Err(format!("Volume '{}' is currently mounted. Unmount it first.", name));
                }
                Ok(name.to_string())
            }
            None => Err(format!("Volume '{}' not found", name)),
        }
    }

    pub async fn list_volumes(&self) -> Vec<Volume> {
        let volumes = self.volumes.read().await;
        volumes.values().cloned().collect()
    }

    pub async fn mount_volume(&self, name: &str, mount_point: PathBuf) -> Result<(String, PathBuf), String> {
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
    }

    pub async fn unmount_volume(&self, name: &str) -> Result<String, String> {
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
    }

    pub async fn modify_volume(&self, name: &str, new_name: Option<String>) -> Result<Volume, String> {
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
    }

    pub async fn get_mounted_volumes(&self) -> Vec<Volume> {
        let volumes = self.volumes.read().await;
        volumes.values()
            .filter(|v| v.is_mounted)
            .cloned()
            .collect()
    }
}