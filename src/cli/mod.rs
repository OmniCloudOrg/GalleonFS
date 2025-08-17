pub mod client;

use clap::{Parser, Subcommand};
use client::DaemonClient;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "galleonfs")]
#[command(about = "Manage volumes in the GalleonFS virtual file system", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Daemon address (default: 127.0.0.1:8888)
    #[arg(long, default_value = "127.0.0.1:8888")]
    daemon_address: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon server
    Daemon {
        /// Address to bind the daemon to
        #[arg(long, default_value = "127.0.0.1:8888")]
        bind: String,
    },
    /// List all volumes
    List,
    /// Create a new volume
    Create {
        /// Name of the volume
        name: String,
    },
    /// Delete a volume
    Delete {
        /// Name of the volume
        name: String,
    },
    /// Mount a volume
    Mount {
        /// Name of the volume
        name: String,
        /// Mount point
        mount_point: PathBuf,
    },
    /// Unmount a volume
    Unmount {
        /// Name of the volume
        name: String,
    },
    /// Modify a volume (rename)
    Modify {
        /// Current name of the volume
        name: String,
        /// New name for the volume
        #[arg(long)]
        new_name: Option<String>,
    },
}

pub async fn run_cli() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Daemon { bind } => {
            use crate::daemon::server::DaemonServer;
            println!("🚀 Starting GalleonFS daemon on {}", bind);
            let server = DaemonServer::new().await?;
            server.start(bind).await?;
        }
        Commands::List => {
            let client = DaemonClient::new(cli.daemon_address);
            client.list_volumes().await?;
        }
        Commands::Create { name } => {
            let client = DaemonClient::new(cli.daemon_address);
            client.create_volume(name.clone()).await?;
        }
        Commands::Delete { name } => {
            let client = DaemonClient::new(cli.daemon_address);
            client.delete_volume(name.clone()).await?;
        }
        Commands::Mount { name, mount_point } => {
            let client = DaemonClient::new(cli.daemon_address);
            client.mount_volume(name.clone(), mount_point.clone()).await?;
        }
        Commands::Unmount { name } => {
            let client = DaemonClient::new(cli.daemon_address);
            client.unmount_volume(name.clone()).await?;
        }
        Commands::Modify { name, new_name } => {
            let client = DaemonClient::new(cli.daemon_address);
            client.modify_volume(name.clone(), new_name.clone()).await?;
        }
    }
    
    Ok(())
}