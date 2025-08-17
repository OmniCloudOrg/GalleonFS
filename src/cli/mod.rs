use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "galleonfs")]
#[command(about = "Manage volumes in the GalleonFS virtual file system", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
        mount_point: String,
    },
    /// Unmount a volume
    Unmount {
        /// Name of the volume
        name: String,
    },
}

pub fn run_cli() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::List => {
            println!("Listing all volumes...");
            // TODO: Implement listing logic
        }
        Commands::Create { name } => {
            println!("Creating volume: {}", name);
            // TODO: Implement creation logic
        }
        Commands::Delete { name } => {
            println!("Deleting volume: {}", name);
            // TODO: Implement deletion logic
        }
        Commands::Mount { name, mount_point } => {
            println!("Mounting volume '{}' at '{}'", name, mount_point);
            // TODO: Implement mount logic
        }
        Commands::Unmount { name } => {
            println!("Unmounting volume: {}", name);
            // TODO: Implement unmount logic
        }
    }
}