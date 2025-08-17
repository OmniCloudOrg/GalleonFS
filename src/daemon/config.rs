use crate::core::types::volume::Volume;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub volumes: HashMap<String, Volume>,
    pub version: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            volumes: HashMap::new(),
            version: "0.1.0".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let config_dir = Self::get_config_directory()?;
        let config_path = config_dir.join("galleonfs").join("volumes.json");
        
        Ok(Self { config_path })
    }

    fn get_config_directory() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        #[cfg(target_os = "windows")]
        {
            dirs::data_dir()
                .ok_or("Failed to get Windows AppData directory".into())
        }
        
        #[cfg(target_os = "macos")]
        {
            dirs::data_dir()
                .ok_or("Failed to get macOS Application Support directory".into())
        }
        
        #[cfg(target_os = "linux")]
        {
            dirs::data_dir()
                .ok_or("Failed to get Linux data directory".into())
        }
        
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Err("Unsupported platform for config directory".into())
        }
    }

    pub async fn load_config(&self) -> DaemonConfig {
        match self.try_load_config().await {
            Ok(config) => {
                info!("Loaded configuration from '{}'", self.config_path.display());
                config
            }
            Err(e) => {
                warn!("Failed to load configuration: {}. Using default config.", e);
                DaemonConfig::default()
            }
        }
    }

    async fn try_load_config(&self) -> Result<DaemonConfig, Box<dyn std::error::Error + Send + Sync>> {
        if !self.config_path.exists() {
            return Ok(DaemonConfig::default());
        }

        let content = fs::read_to_string(&self.config_path).await?;
        let config: DaemonConfig = serde_json::from_str(&content)?;
        
        // Validate config version compatibility
        if config.version != "0.1.0" {
            warn!("Config version mismatch. Expected 0.1.0, found {}. Using default config.", config.version);
            return Ok(DaemonConfig::default());
        }

        Ok(config)
    }

    pub async fn save_config(&self, config: &DaemonConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Ensure the parent directory exists
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let content = serde_json::to_string_pretty(config)?;
        fs::write(&self.config_path, content).await?;
        
        info!("Saved configuration to '{}'", self.config_path.display());
        Ok(())
    }

    pub async fn backup_config(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.config_path.exists() {
            return Ok(());
        }

        let backup_path = self.config_path.with_extension("json.backup");
        fs::copy(&self.config_path, &backup_path).await?;
        
        info!("Created config backup at '{}'", backup_path.display());
        Ok(())
    }

    pub fn get_config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub async fn migrate_if_needed(&self, config: &mut DaemonConfig) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Future migrations can be implemented here
        // Return true if migration was performed
        
        // For now, just ensure all volumes have valid mount states
        let mut changed = false;
        for volume in config.volumes.values_mut() {
            if volume.is_mounted && volume.mount_point.is_some() {
                // Check if mount point still exists
                if let Some(ref mount_point) = volume.mount_point {
                    if !mount_point.exists() {
                        warn!("Mount point '{}' for volume '{}' no longer exists. Marking as unmounted.", 
                            mount_point.display(), volume.name);
                        volume.is_mounted = false;
                        changed = true;
                    }
                }
            }
        }

        if changed {
            info!("Configuration migrated and updated");
        }

        Ok(changed)
    }
}