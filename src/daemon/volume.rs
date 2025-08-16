use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    id: Uuid,
    name: String,
    mount_path: PathBuf,
    created_at: DateTime<Utc>,
    last_accessed: DateTime<Utc>,
    total_events: u64,
    is_active: bool,
}

impl Volume {
    pub fn new(name: String, mount_path: PathBuf) -> Result<Self> {
        if !mount_path.exists() {
            return Err(anyhow::anyhow!("Mount path does not exist: {:?}", mount_path));
        }

        if !mount_path.is_dir() {
            return Err(anyhow::anyhow!("Mount path is not a directory: {:?}", mount_path));
        }

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            name,
            mount_path,
            created_at: now,
            last_accessed: now,
            total_events: 0,
            is_active: true,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn mount_path(&self) -> &PathBuf {
        &self.mount_path
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn last_accessed(&self) -> DateTime<Utc> {
        self.last_accessed
    }

    pub fn total_events(&self) -> u64 {
        self.total_events
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn update_last_accessed(&mut self) {
        self.last_accessed = Utc::now();
    }

    pub fn increment_event_count(&mut self) {
        self.total_events += 1;
        self.update_last_accessed();
    }

    pub fn activate(&mut self) {
        self.is_active = true;
    }

    pub fn deactivate(&mut self) {
        self.is_active = false;
    }

    pub fn get_mount_path_display(&self) -> String {
        self.mount_path.display().to_string()
    }

    pub fn get_size_info(&self) -> Result<VolumeSize> {
        use std::fs;

        fn dir_size(path: &PathBuf) -> Result<u64> {
            let mut total_size = 0;
            
            if path.is_dir() {
                for entry in fs::read_dir(path)? {
                    let entry = entry?;
                    let path = entry.path();
                    
                    if path.is_dir() {
                        total_size += dir_size(&path)?;
                    } else {
                        total_size += entry.metadata()?.len();
                    }
                }
            }
            
            Ok(total_size)
        }

        let total_bytes = dir_size(&self.mount_path)?;
        Ok(VolumeSize::new(total_bytes))
    }

    pub fn format_summary(&self) -> String {
        let size_info = self.get_size_info()
            .map(|s| s.human_readable())
            .unwrap_or_else(|_| "Unknown".to_string());

        format!(
            "Volume '{}' [{}]\n  Path: {}\n  Created: {}\n  Events: {}\n  Size: {}\n  Status: {}",
            self.name,
            self.id,
            self.get_mount_path_display(),
            self.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
            self.total_events,
            size_info,
            if self.is_active { "Active" } else { "Inactive" }
        )
    }
}

#[derive(Debug, Clone)]
pub struct VolumeSize {
    bytes: u64,
}

impl VolumeSize {
    pub fn new(bytes: u64) -> Self {
        Self { bytes }
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn human_readable(&self) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = self.bytes as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", self.bytes, UNITS[unit_index])
        } else {
            format!("{:.2} {}", size, UNITS[unit_index])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_volume_creation() {
        let temp_dir = TempDir::new().unwrap();
        let volume = Volume::new("test_volume".to_string(), temp_dir.path().to_path_buf()).unwrap();

        assert_eq!(volume.name(), "test_volume");
        assert_eq!(volume.mount_path(), temp_dir.path());
        assert!(volume.is_active());
        assert_eq!(volume.total_events(), 0);
    }

    #[test]
    fn test_volume_creation_nonexistent_path() {
        let result = Volume::new("test_volume".to_string(), PathBuf::from("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_volume_size_human_readable() {
        assert_eq!(VolumeSize::new(512).human_readable(), "512 B");
        assert_eq!(VolumeSize::new(1024).human_readable(), "1.00 KB");
        assert_eq!(VolumeSize::new(1536).human_readable(), "1.50 KB");
        assert_eq!(VolumeSize::new(1048576).human_readable(), "1.00 MB");
        assert_eq!(VolumeSize::new(1073741824).human_readable(), "1.00 GB");
    }

    #[test]
    fn test_volume_event_counting() {
        let temp_dir = TempDir::new().unwrap();
        let mut volume = Volume::new("test_volume".to_string(), temp_dir.path().to_path_buf()).unwrap();

        assert_eq!(volume.total_events(), 0);
        
        volume.increment_event_count();
        assert_eq!(volume.total_events(), 1);
        
        volume.increment_event_count();
        assert_eq!(volume.total_events(), 2);
    }
}