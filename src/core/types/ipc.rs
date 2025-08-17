use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;
use super::volume::Volume;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    CreateVolume { name: String },
    DeleteVolume { name: String },
    ListVolumes,
    MountVolume { name: String, mount_point: PathBuf },
    UnmountVolume { name: String },
    ModifyVolume { name: String, new_name: Option<String> },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    Success,
    VolumeCreated { volume: Volume },
    VolumeDeleted { name: String },
    VolumeMounted { name: String, mount_point: PathBuf },
    VolumeUnmounted { name: String },
    VolumeModified { volume: Volume },
    VolumesList { volumes: Vec<Volume> },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub id: Uuid,
    pub request: IpcRequest,
}

impl IpcMessage {
    pub fn new(request: IpcRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            request,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponseMessage {
    pub id: Uuid,
    pub response: IpcResponse,
}

impl IpcResponseMessage {
    pub fn new(id: Uuid, response: IpcResponse) -> Self {
        Self { id, response }
    }
}