use std::sync::Arc;
use tracing::{info, error};
use galleon_common::{config::NodeConfig, error::GalleonResult};

mod collector;
mod analytics;
mod alerting;
mod dashboard;
mod ml_optimizer;
mod capacity;
mod volume_analytics;
mod reporting;

#[tokio::main]
async fn main() -> GalleonResult<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("📊 Starting GalleonFS Metrics Service");
    info!("🔗 Features: metrics collection, analytics, ML optimization, capacity planning");

    let config = NodeConfig::load().await?;
    
    info!("✅ GalleonFS Metrics Service ready");
    
    tokio::signal::ctrl_c().await?;
    info!("🛑 Metrics Service shutdown");
    
    Ok(())
}