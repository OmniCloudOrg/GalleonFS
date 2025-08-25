use anyhow::Result;
use clap::Parser;
use galleon_common::config::StorageNodeConfig;
use std::sync::Arc;
use tokio::signal;
use tracing::{info, error};

mod storage_engine;
mod device_manager;
mod chunk_manager;
mod replication;
mod erasure_coding;
mod encryption;
mod compression;
mod io_engine;
mod block_allocator;
mod consistency;
mod striping;
mod tiering;
mod recovery;
mod server;

use crate::storage_engine::StorageEngine;
use crate::server::StorageNodeServer;

#[derive(Parser, Debug)]
#[command(name = "galleon-storage-node")]
#[command(about = "High-performance distributed storage node with Linux optimization")]
struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "/etc/galleonfs/storage-node.toml")]
    config: String,
    
    /// Node ID (if not in config)
    #[arg(long)]
    node_id: Option<String>,
    
    /// Bind address for gRPC server
    #[arg(long, default_value = "0.0.0.0:9001")]
    bind_address: String,
    
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
    
    /// Enable RDMA support
    #[arg(long)]
    enable_rdma: bool,
    
    /// Enable NUMA optimization
    #[arg(long)]
    enable_numa: bool,
    
    /// Enable hardware acceleration
    #[arg(long, default_value = "true")]
    enable_hw_accel: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize tracing
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(if cli.debug {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)?;
    
    info!("Starting GalleonFS Storage Node");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));
    info!("Build: {} {}", env!("VERGEN_GIT_BRANCH"), env!("VERGEN_GIT_SHA"));
    
    // Load configuration
    let mut config = StorageNodeConfig::from_file(&cli.config).await
        .unwrap_or_else(|_| {
            info!("Config file not found, using defaults");
            StorageNodeConfig::default()
        });
    
    // Override config with CLI arguments
    if let Some(node_id) = cli.node_id {
        config.node_id = node_id;
    }
    config.bind_address = cli.bind_address.parse()?;
    config.enable_rdma = cli.enable_rdma;
    config.enable_numa = cli.enable_numa;
    config.enable_hw_accel = cli.enable_hw_accel;
    
    info!("Node ID: {}", config.node_id);
    info!("Bind address: {}", config.bind_address);
    info!("RDMA enabled: {}", config.enable_rdma);
    info!("NUMA enabled: {}", config.enable_numa);
    info!("Hardware acceleration: {}", config.enable_hw_accel);
    
    // Initialize storage engine
    info!("Initializing storage engine...");
    let storage_engine = Arc::new(
        StorageEngine::new(config.clone()).await?
    );
    
    // Initialize and start the gRPC server
    info!("Starting gRPC server on {}", config.bind_address);
    let server = StorageNodeServer::new(storage_engine.clone(), config.clone()).await?;
    
    // Start the server in a separate task
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.serve().await {
            error!("Server error: {}", e);
        }
    });
    
    // Initialize metrics collection
    storage_engine.start_metrics_collection().await?;
    
    info!("GalleonFS Storage Node started successfully");
    info!("Ready to serve storage requests");
    
    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = server_handle => {
            error!("Server task completed unexpectedly");
        }
    }
    
    info!("Shutting down storage engine...");
    storage_engine.shutdown().await?;
    
    info!("GalleonFS Storage Node shutdown complete");
    Ok(())
}