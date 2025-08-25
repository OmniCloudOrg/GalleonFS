//! gRPC server for storage node API

use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, error};

use galleon_common::config::StorageNodeConfig;
use crate::storage_engine::StorageEngine;

/// Storage node gRPC server
pub struct StorageNodeServer {
    storage_engine: Arc<StorageEngine>,
    config: StorageNodeConfig,
}

impl StorageNodeServer {
    /// Create a new storage node server
    pub async fn new(
        storage_engine: Arc<StorageEngine>,
        config: StorageNodeConfig,
    ) -> Result<Self> {
        Ok(Self {
            storage_engine,
            config,
        })
    }
    
    /// Start serving gRPC requests
    pub async fn serve(self) -> Result<()> {
        let addr: SocketAddr = self.config.bind_address;
        
        info!("Starting gRPC server on {}", addr);
        
        // This would implement the actual gRPC service
        // For now, just start a basic server
        Server::builder()
            .serve(addr)
            .await?;
        
        Ok(())
    }
}

// TODO: Implement actual gRPC service definitions
// This would include:
// - Volume management RPCs
// - Chunk operations
// - Health checks
// - Metrics endpoints