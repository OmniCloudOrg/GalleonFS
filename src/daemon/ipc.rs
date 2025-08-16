use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;
use crate::daemon::Volume;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    CreateVolume { name: String, mount_path: PathBuf },
    ListVolumes,
    RemoveVolume { volume_id: Uuid },
    GetVolumeByName { name: String },
    Ping,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    VolumeCreated { volume_id: Uuid },
    VolumesList { volumes: Vec<Volume> },
    VolumeRemoved,
    Volume { volume: Option<Volume> },
    Pong,
    ShutdownComplete,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub id: Uuid,
    pub request: IpcRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcReply {
    pub id: Uuid,
    pub response: IpcResponse,
}

impl IpcMessage {
    pub fn new(request: IpcRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            request,
        }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}

impl IpcReply {
    pub fn new(id: Uuid, response: IpcResponse) -> Self {
        Self { id, response }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}