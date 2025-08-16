use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsEvent {
    pub volume_id: Uuid,
    pub event_type: VfsEventType,
    pub path: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VfsEventType {
    FileCreated,
    FileModified,
    FileDeleted,
    FileRenamed { from: String, to: String },
    DirectoryCreated,
    DirectoryDeleted,
    DirectoryRenamed { from: String, to: String },
    Unknown,
}

impl VfsEvent {
    pub fn new(volume_id: Uuid, event_type: VfsEventType, path: String) -> Self {
        Self {
            volume_id,
            event_type,
            path,
            timestamp: chrono::Utc::now(),
        }
    }
}