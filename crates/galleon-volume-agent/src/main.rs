use std::sync::Arc;
use tracing::{info, error};
use galleon_common::{config::NodeConfig, error::GalleonResult};

mod fuse_fs;
mod mount;
mod csi;
mod security;
mod qos;
mod snapshot;
mod large_volume;
mod backup;

use fuse_fs::GalleonFuseFS;

#[tokio::main]
async fn main() -> GalleonResult<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🗄️  Starting GalleonFS Volume Agent");
    info!("🔗 Features: FUSE filesystem, CSI interface, snapshots, large volumes");

    let config = NodeConfig::load().await?;
    
    // Initialize FUSE filesystem
    let fuse_fs = Arc::new(GalleonFuseFS::new(&config).await?);
    
    // Start volume management services
    info!("✅ GalleonFS Volume Agent ready");
    
    tokio::signal::ctrl_c().await?;
    info!("🛑 Volume Agent shutdown");
    
    Ok(())
}