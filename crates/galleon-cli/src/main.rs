use clap::{Parser, Subcommand};
use tracing::{info, error};
use galleon_common::{config::NodeConfig, error::GalleonResult};

mod commands;
mod client;
mod config;
mod output;

use commands::*;

#[derive(Parser)]
#[command(name = "galleon")]
#[command(about = "GalleonFS Enterprise Distributed Storage CLI")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Volume management operations
    Volume(volume::VolumeArgs),
    /// Cluster management operations  
    Cluster(cluster::ClusterArgs),
    /// Storage operations
    Storage(storage::StorageArgs),
    /// Device management
    Device(device::DeviceArgs),
    /// Backup operations
    Backup(backup::BackupArgs),
    /// Administrative operations
    Admin(admin::AdminArgs),
}

#[tokio::main]
async fn main() -> GalleonResult<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Volume(args) => volume::handle_volume_command(args).await,
        Commands::Cluster(args) => cluster::handle_cluster_command(args).await,
        Commands::Storage(args) => storage::handle_storage_command(args).await,
        Commands::Device(args) => device::handle_device_command(args).await,
        Commands::Backup(args) => backup::handle_backup_command(args).await,
        Commands::Admin(args) => admin::handle_admin_command(args).await,
    }
}