use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub id: Uuid,
    pub name: String,
    pub mount_point: Option<PathBuf>,
    pub is_mounted: bool,
    pub created_at: std::time::SystemTime,
}

impl Volume {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            mount_point: None,
            is_mounted: false,
            created_at: std::time::SystemTime::now(),
        }
    }

    pub fn mount(&mut self, mount_point: PathBuf) {
        self.mount_point = Some(mount_point);
        self.is_mounted = true;
    }

    pub fn unmount(&mut self) {
        self.is_mounted = false;
    }
}