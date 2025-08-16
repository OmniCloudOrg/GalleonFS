use anyhow::Result;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::{Volume, ipc::{IpcMessage, IpcReply, IpcRequest, IpcResponse}, server::DEFAULT_DAEMON_PORT};

pub struct DaemonClient {
    address: String,
}

impl DaemonClient {
    pub fn new(port: Option<u16>) -> Self {
        let port = port.unwrap_or(DEFAULT_DAEMON_PORT);
        Self {
            address: format!("127.0.0.1:{}", port),
        }
    }

    pub async fn is_daemon_running(&self) -> bool {
        match self.ping().await {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub async fn ping(&self) -> Result<()> {
        let response = self.send_request(IpcRequest::Ping).await?;
        match response.response {
            IpcResponse::Pong => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!("Ping failed: {}", message)),
            _ => Err(anyhow::anyhow!("Unexpected response to ping")),
        }
    }

    pub async fn create_volume(&self, name: String, mount_path: PathBuf) -> Result<Uuid> {
        let response = self.send_request(IpcRequest::CreateVolume { name, mount_path }).await?;
        match response.response {
            IpcResponse::VolumeCreated { volume_id } => Ok(volume_id),
            IpcResponse::Error { message } => Err(anyhow::anyhow!("Failed to create volume: {}", message)),
            _ => Err(anyhow::anyhow!("Unexpected response to create volume")),
        }
    }

    pub async fn list_volumes(&self) -> Result<Vec<Volume>> {
        let response = self.send_request(IpcRequest::ListVolumes).await?;
        match response.response {
            IpcResponse::VolumesList { volumes } => Ok(volumes),
            IpcResponse::Error { message } => Err(anyhow::anyhow!("Failed to list volumes: {}", message)),
            _ => Err(anyhow::anyhow!("Unexpected response to list volumes")),
        }
    }

    pub async fn remove_volume(&self, volume_id: Uuid) -> Result<()> {
        let response = self.send_request(IpcRequest::RemoveVolume { volume_id }).await?;
        match response.response {
            IpcResponse::VolumeRemoved => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!("Failed to remove volume: {}", message)),
            _ => Err(anyhow::anyhow!("Unexpected response to remove volume")),
        }
    }

    pub async fn get_volume_by_name(&self, name: String) -> Result<Option<Volume>> {
        let response = self.send_request(IpcRequest::GetVolumeByName { name }).await?;
        match response.response {
            IpcResponse::Volume { volume } => Ok(volume),
            IpcResponse::Error { message } => Err(anyhow::anyhow!("Failed to get volume: {}", message)),
            _ => Err(anyhow::anyhow!("Unexpected response to get volume")),
        }
    }

    pub async fn shutdown_daemon(&self) -> Result<()> {
        let response = self.send_request(IpcRequest::Shutdown).await?;
        match response.response {
            IpcResponse::ShutdownComplete => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!("Failed to shutdown daemon: {}", message)),
            _ => Err(anyhow::anyhow!("Unexpected response to shutdown")),
        }
    }

    async fn send_request(&self, request: IpcRequest) -> Result<IpcReply> {
        info!("📞 Connecting to daemon at {}", self.address);
        
        let mut stream = TcpStream::connect(&self.address).await
            .map_err(|e| anyhow::anyhow!("Failed to connect to daemon at {}: {}. Is the daemon running?", self.address, e))?;

        let message = IpcMessage::new(request);
        let message_json = format!("{}\n", message.to_json()?);

        info!("📤 Sending request: {}", message_json.trim());

        stream.write_all(message_json.as_bytes()).await?;

        let (reader, _writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut response_line = String::new();

        reader.read_line(&mut response_line).await?;

        info!("📨 Received response: {}", response_line.trim());

        if response_line.trim().is_empty() {
            return Err(anyhow::anyhow!("Empty response from daemon"));
        }

        let reply = IpcReply::from_json(response_line.trim())?;
        Ok(reply)
    }
}

impl Default for DaemonClient {
    fn default() -> Self {
        Self::new(None)
    }
}