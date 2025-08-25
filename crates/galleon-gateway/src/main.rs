use std::sync::Arc;
use tracing::{info, error};
use galleon_common::{config::NodeConfig, error::GalleonResult};

mod s3_api;
mod block_api;
mod grpc_api;
mod rest_api;
mod auth;
mod proxy;
mod rate_limit;
mod tenant;

#[tokio::main]
async fn main() -> GalleonResult<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🌐 Starting GalleonFS API Gateway");
    info!("🔗 Features: S3-compatible API, native block API, gRPC, multi-tenancy");

    let config = NodeConfig::load().await?;
    
    info!("✅ GalleonFS API Gateway ready");
    
    tokio::signal::ctrl_c().await?;
    info!("🛑 API Gateway shutdown");
    
    Ok(())
}