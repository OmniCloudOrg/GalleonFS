use crate::{BlockData, ReplicationStrategy, StorageEngine, WriteConcern};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplicationMessage {
    WriteBlock(BlockData),
    WriteAck { block_id: u64, success: bool },
    CreateVolume(crate::Volume),
    VolumeCreated { volume_id: uuid::Uuid, success: bool },
    Heartbeat,
    HeartbeatAck,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationRequest {
    pub id: Uuid,
    pub message: ReplicationMessage,
}

impl ReplicationRequest {
    pub fn new(message: ReplicationMessage) -> Self {
        Self {
            id: Uuid::new_v4(),
            message,
        }
    }
}

#[derive(Clone)]
pub struct ReplicationService {
    storage_engine: Arc<dyn StorageEngine>,
    peer_addresses: Arc<tokio::sync::RwLock<Vec<String>>>,
    async_sender: Option<mpsc::UnboundedSender<BlockData>>,
}

impl ReplicationService {
    pub fn new(storage_engine: Arc<dyn StorageEngine>, peer_addresses: Vec<String>) -> Self {
        Self {
            storage_engine,
            peer_addresses: Arc::new(tokio::sync::RwLock::new(peer_addresses)),
            async_sender: None,
        }
    }

    /// Update the list of peer addresses for replication
    pub async fn update_peer_addresses(&self, new_peer_addresses: Vec<String>) {
        let mut peers = self.peer_addresses.write().await;
        *peers = new_peer_addresses;
        info!("Updated replication peer addresses: {:?}", *peers);
    }

    /// Add a new peer address
    pub async fn add_peer_address(&self, peer_address: String) {
        let mut peers = self.peer_addresses.write().await;
        if !peers.contains(&peer_address) {
            peers.push(peer_address.clone());
            info!("Added replication peer: {}", peer_address);
        }
    }

    /// Remove a peer address
    pub async fn remove_peer_address(&self, peer_address: &str) {
        let mut peers = self.peer_addresses.write().await;
        peers.retain(|addr| addr != peer_address);
        info!("Removed replication peer: {}", peer_address);
    }

    /// Get current peer addresses
    pub async fn get_peer_addresses(&self) -> Vec<String> {
        self.peer_addresses.read().await.clone()
    }

    pub async fn run(&self, bind_address: String, strategy: ReplicationStrategy) -> Result<()> {
        let listener = TcpListener::bind(&bind_address).await?;
        info!("Replication service listening on {}", bind_address);

        match strategy {
            ReplicationStrategy::Synchronous => {
                self.run_synchronous_server(listener).await
            }
            ReplicationStrategy::Asynchronous => {
                self.run_asynchronous_server(listener).await
            }
        }
    }

    async fn run_synchronous_server(&self, listener: TcpListener) -> Result<()> {
        loop {
            match listener.accept().await {
                Ok((mut stream, addr)) => {
                    info!("Accepted synchronous connection from {}", addr);
                    let storage_engine = self.storage_engine.clone();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_synchronous_connection(&mut stream, storage_engine).await {
                            error!("Error handling synchronous connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    async fn run_asynchronous_server(&self, listener: TcpListener) -> Result<()> {
        let (tx, mut rx) = mpsc::unbounded_channel::<BlockData>();
        
        let storage_engine = self.storage_engine.clone();
        tokio::spawn(async move {
            while let Some(block_data) = rx.recv().await {
                if let Err(e) = storage_engine
                    .write_block(block_data.volume_id, block_data.block_id, &block_data.data)
                    .await
                {
                    error!("Failed to write block asynchronously: {}", e);
                }
            }
        });

        loop {
            match listener.accept().await {
                Ok((mut stream, addr)) => {
                    info!("Accepted asynchronous connection from {}", addr);
                    let storage_engine = self.storage_engine.clone();
                    let tx = tx.clone();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_asynchronous_connection(&mut stream, storage_engine, tx).await {
                            error!("Error handling asynchronous connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    async fn handle_synchronous_connection(
        stream: &mut TcpStream,
        storage_engine: Arc<dyn StorageEngine>,
    ) -> Result<()> {
        let mut buffer = vec![0u8; 8192];
        
        loop {
            let n = stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            match bincode::deserialize::<ReplicationRequest>(&buffer[..n]) {
                Ok(request) => {
                    match request.message {
                        ReplicationMessage::WriteBlock(block_data) => {
                            let success = if block_data.verify_checksum() {
                                storage_engine
                                    .write_block(
                                        block_data.volume_id,
                                        block_data.block_id,
                                        &block_data.data,
                                    )
                                    .await
                                    .is_ok()
                            } else {
                                warn!("Checksum verification failed for block {}", block_data.block_id);
                                false
                            };

                            let ack = ReplicationRequest::new(ReplicationMessage::WriteAck {
                                block_id: block_data.block_id,
                                success,
                            });

                            let ack_data = bincode::serialize(&ack)?;
                            stream.write_all(&ack_data).await?;
                        }
                        ReplicationMessage::CreateVolume(volume) => {
                            info!("Received volume creation request for volume {}", volume.id);
                            let mut volume_copy = volume.clone();
                            let create_result = storage_engine.create_volume(&mut volume_copy).await;
                            let success = create_result.is_ok();

                            let ack = ReplicationRequest::new(ReplicationMessage::VolumeCreated {
                                volume_id: volume.id,
                                success,
                            });

                            let ack_data = bincode::serialize(&ack)?;
                            stream.write_all(&ack_data).await?;

                            if success {
                                info!("Successfully created replicated volume {}", volume.id);
                            } else {
                                warn!("Failed to create replicated volume {}", volume.id);
                            }
                        }
                        ReplicationMessage::Heartbeat => {
                            let ack = ReplicationRequest::new(ReplicationMessage::HeartbeatAck);
                            let ack_data = bincode::serialize(&ack)?;
                            stream.write_all(&ack_data).await?;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    error!("Failed to deserialize replication request: {}", e);
                }
            }
        }

        Ok(())
    }

    async fn handle_asynchronous_connection(
        stream: &mut TcpStream,
        _storage_engine: Arc<dyn StorageEngine>,
        tx: mpsc::UnboundedSender<BlockData>,
    ) -> Result<()> {
        let mut buffer = vec![0u8; 8192];
        
        loop {
            let n = stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            match bincode::deserialize::<ReplicationRequest>(&buffer[..n]) {
                Ok(request) => {
                    match request.message {
                        ReplicationMessage::WriteBlock(block_data) => {
                            if block_data.verify_checksum() {
                                if let Err(e) = tx.send(block_data) {
                                    error!("Failed to queue block for async processing: {}", e);
                                }
                            } else {
                                warn!("Checksum verification failed for block {}", block_data.block_id);
                            }
                        }
                        ReplicationMessage::Heartbeat => {
                            let ack = ReplicationRequest::new(ReplicationMessage::HeartbeatAck);
                            let ack_data = bincode::serialize(&ack)?;
                            stream.write_all(&ack_data).await?;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    error!("Failed to deserialize replication request: {}", e);
                }
            }
        }

        Ok(())
    }

    pub async fn replicate_synchronously(
        &self,
        block_data: &BlockData,
        write_concern: WriteConcern,
    ) -> Result<()> {
        let peer_addresses = self.peer_addresses.read().await;
        let required_replicas = match write_concern {
            WriteConcern::WriteReplicated => 1,
            WriteConcern::WriteDistributed => peer_addresses.len().max(1),
            _ => 0,
        };

        if required_replicas == 0 {
            return Ok(());
        }

        let mut successful_replicas = 0;
        let request = ReplicationRequest::new(ReplicationMessage::WriteBlock(block_data.clone()));
        let request_data = bincode::serialize(&request)?;

        for peer_address in peer_addresses.iter() {
            match TcpStream::connect(peer_address).await {
                Ok(mut stream) => {
                    if let Err(e) = stream.write_all(&request_data).await {
                        error!("Failed to send data to peer {}: {}", peer_address, e);
                        continue;
                    }

                    let mut buffer = vec![0u8; 1024];
                    match stream.read(&mut buffer).await {
                        Ok(n) if n > 0 => {
                            match bincode::deserialize::<ReplicationRequest>(&buffer[..n]) {
                                Ok(response) => {
                                    if let ReplicationMessage::WriteAck { success, .. } = response.message {
                                        if success {
                                            successful_replicas += 1;
                                            info!("Successfully replicated to peer {}", peer_address);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to deserialize response from {}: {}", peer_address, e);
                                }
                            }
                        }
                        Ok(_) => {
                            warn!("Received empty response from peer {}", peer_address);
                        }
                        Err(e) => {
                            error!("Failed to read response from {}: {}", peer_address, e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to connect to peer {}: {}", peer_address, e);
                }
            }

            if successful_replicas >= required_replicas {
                break;
            }
        }

        if successful_replicas < required_replicas {
            return Err(anyhow::anyhow!(
                "Failed to replicate to required number of peers. Required: {}, Successful: {}",
                required_replicas,
                successful_replicas
            ));
        }

        Ok(())
    }

    pub async fn replicate_asynchronously(&self, block_data: &BlockData) -> Result<()> {
        let request = ReplicationRequest::new(ReplicationMessage::WriteBlock(block_data.clone()));
        let request_data = bincode::serialize(&request)?;

        let peer_addresses = self.peer_addresses.read().await;
        for peer_address in peer_addresses.iter() {
            let peer_address = peer_address.clone();
            let request_data = request_data.clone();
            
            tokio::spawn(async move {
                match TcpStream::connect(&peer_address).await {
                    Ok(mut stream) => {
                        if let Err(e) = stream.write_all(&request_data).await {
                            error!("Failed to send async data to peer {}: {}", peer_address, e);
                        } else {
                            info!("Asynchronously sent data to peer {}", peer_address);
                        }
                    }
                    Err(e) => {
                        error!("Failed to connect to peer {} for async replication: {}", peer_address, e);
                    }
                }
            });
        }

        Ok(())
    }

    pub async fn replicate_volume_creation(&self, volume: &crate::Volume) -> Result<()> {
        let peer_addresses = self.peer_addresses.read().await;
        if peer_addresses.is_empty() {
            return Ok(());
        }

        let request = ReplicationRequest::new(ReplicationMessage::CreateVolume(volume.clone()));
        let request_data = bincode::serialize(&request)?;

        for peer_address in peer_addresses.iter() {
            match TcpStream::connect(peer_address).await {
                Ok(mut stream) => {
                    if let Err(e) = stream.write_all(&request_data).await {
                        error!("Failed to send volume creation to peer {}: {}", peer_address, e);
                        continue;
                    }

                    let mut buffer = vec![0u8; 1024];
                    match stream.read(&mut buffer).await {
                        Ok(n) if n > 0 => {
                            match bincode::deserialize::<ReplicationRequest>(&buffer[..n]) {
                                Ok(response) => {
                                    if let ReplicationMessage::VolumeCreated { success, .. } = response.message {
                                        if success {
                                            info!("Successfully replicated volume {} to peer {}", volume.id, peer_address);
                                        } else {
                                            warn!("Failed to replicate volume {} to peer {}", volume.id, peer_address);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to deserialize volume creation response from {}: {}", peer_address, e);
                                }
                            }
                        }
                        Ok(_) => {
                            warn!("Received empty response from peer {}", peer_address);
                        }
                        Err(e) => {
                            error!("Failed to read volume creation response from {}: {}", peer_address, e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to connect to peer {} for volume replication: {}", peer_address, e);
                }
            }
        }

        Ok(())
    }
}