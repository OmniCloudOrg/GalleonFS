//! Main entry point for the GalleonFS Coordinator service

use galleon_common::{
    config::{ConfigLoader, GalleonConfig},
    error::Result,
};
use galleon_coordinator::GalleonCoordinator;
use std::path::PathBuf;
use clap::{Arg, Command};
use tracing::{info, error};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Parse command line arguments
    let matches = Command::new("galleon-coordinator")
        .version("1.0.0")
        .about("GalleonFS Coordinator - Control plane service for distributed storage")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Configuration file path")
                .default_value("coordinator.toml")
        )
        .arg(
            Arg::new("bind-address")
                .short('b')
                .long("bind-address")
                .value_name("ADDRESS")
                .help("Bind address for the coordinator service")
                .default_value("0.0.0.0:8080")
        )
        .arg(
            Arg::new("data-dir")
                .short('d')
                .long("data-dir")
                .value_name("DIR")
                .help("Data directory for coordinator state")
                .default_value("./coordinator_data")
        )
        .arg(
            Arg::new("cluster-name")
                .long("cluster-name")
                .value_name("NAME")
                .help("Name of the cluster")
                .default_value("galleonfs-cluster")
        )
        .arg(
            Arg::new("peer-addresses")
                .long("peer-addresses")
                .value_name("ADDRESSES")
                .help("Comma-separated list of peer coordinator addresses")
        )
        .arg(
            Arg::new("node-id")
                .long("node-id")
                .value_name("UUID")
                .help("Unique node identifier (generated if not provided)")
        )
        .arg(
            Arg::new("log-level")
                .long("log-level")
                .value_name("LEVEL")
                .help("Log level")
                .default_value("info")
                .value_parser(["trace", "debug", "info", "warn", "error"])
        )
        .get_matches();

    // Load configuration
    let config_path = PathBuf::from(matches.get_one::<String>("config").unwrap());
    let mut config = if config_path.exists() {
        info!("Loading configuration from {}", config_path.display());
        ConfigLoader::load_from_file(&config_path)?
    } else {
        info!("Using default configuration");
        GalleonConfig::default()
    };

    // Override config with command line arguments
    if let Some(bind_address) = matches.get_one::<String>("bind-address") {
        config.network.bind_address = bind_address.parse()
            .map_err(|e| galleon_common::error::GalleonError::ConfigError(
                format!("Invalid bind address: {}", e)
            ))?;
    }

    if let Some(data_dir) = matches.get_one::<String>("data-dir") {
        config.storage.data_dir = PathBuf::from(data_dir);
    }

    if let Some(cluster_name) = matches.get_one::<String>("cluster-name") {
        config.cluster.name = cluster_name.clone();
    }

    if let Some(peer_addresses) = matches.get_one::<String>("peer-addresses") {
        config.cluster.peer_addresses = peer_addresses
            .split(',')
            .map(|addr| addr.trim().parse())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| galleon_common::error::GalleonError::ConfigError(
                format!("Invalid peer address: {}", e)
            ))?;
    }

    if let Some(node_id) = matches.get_one::<String>("node-id") {
        config.node.id = Some(
            node_id.parse()
                .map_err(|e| galleon_common::error::GalleonError::ConfigError(
                    format!("Invalid node ID: {}", e)
                ))?
        );
    }

    // Ensure data directory exists
    std::fs::create_dir_all(&config.storage.data_dir)
        .map_err(|e| galleon_common::error::GalleonError::IoError(e))?;

    info!("Starting GalleonFS Coordinator");
    info!("Bind address: {}", config.network.bind_address);
    info!("Data directory: {}", config.storage.data_dir.display());
    info!("Cluster name: {}", config.cluster.name);

    // Create and start coordinator
    let coordinator = GalleonCoordinator::new(config).await?;
    
    // Set up signal handling for graceful shutdown
    let coordinator_for_shutdown = coordinator.clone();
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler");
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("Failed to install SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down gracefully");
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, shutting down gracefully");
            }
        }

        if let Err(e) = coordinator_for_shutdown.stop().await {
            error!("Error during shutdown: {}", e);
        }
    });

    // Start the coordinator
    coordinator.start().await?;

    // Keep the coordinator running
    info!("GalleonFS Coordinator is running...");
    
    // Wait indefinitely (the signal handler will trigger shutdown)
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    loop {
        interval.tick().await;
        
        // Log cluster status periodically
        if coordinator.is_leader().await {
            match coordinator.get_cluster_stats().await {
                Ok(stats) => {
                    info!(
                        "Cluster stats: {} nodes, {} volumes, {:.1}% utilization",
                        stats.active_nodes,
                        stats.total_volumes,
                        stats.utilization_percent
                    );
                }
                Err(e) => {
                    error!("Failed to get cluster stats: {}", e);
                }
            }
        }
    }
}

/// Extension trait to add clone capability to coordinator
impl Clone for galleon_coordinator::GalleonCoordinator {
    fn clone(&self) -> Self {
        // This is a simplified clone that shares the same underlying services
        // In a real implementation, you might want a different approach
        Self {
            config: self.config.clone(),
            consensus: self.consensus.clone(),
            placement_engine: self.placement_engine.clone(),
            metadata_store: self.metadata_store.clone(),
            cluster_manager: self.cluster_manager.clone(),
            health_monitor: self.health_monitor.clone(),
            volume_manager: self.volume_manager.clone(),
            topology_manager: self.topology_manager.clone(),
            migration_manager: self.migration_manager.clone(),
            api_server: self.api_server.clone(),
            cluster_state: self.cluster_state.clone(),
        }
    }
}