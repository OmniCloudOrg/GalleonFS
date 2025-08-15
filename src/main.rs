//! GalleonFS Main Entry Point
//!
//! This is the main entry point for GalleonFS that supports both daemon mode
//! and CLI mode operation.

use anyhow::Result;
use clap::{Parser, ValueEnum};
use galleonfs::{
    daemon::{DaemonConfig, GalleonDaemon},
    cli::{Cli as GalleonCli, GalleonClient},
    cluster::{ClusterManager, get_default_node_capabilities},
    storage::FileStorageEngine, 
    PersistenceLevel, 
    ReplicationStrategy, 
};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber;

#[derive(Debug, Clone, ValueEnum)]
enum CliReplicationStrategy {
    Synchronous,
    Asynchronous,
}

impl From<CliReplicationStrategy> for ReplicationStrategy {
    fn from(cli_strategy: CliReplicationStrategy) -> Self {
        match cli_strategy {
            CliReplicationStrategy::Synchronous => ReplicationStrategy::Synchronous,
            CliReplicationStrategy::Asynchronous => ReplicationStrategy::Asynchronous,
        }
    }
}

#[derive(Debug, Clone, ValueEnum)]
enum CliPersistenceLevel {
    Basic,
    Enhanced,
    High,
    Maximum,
}

impl From<CliPersistenceLevel> for PersistenceLevel {
    fn from(cli_level: CliPersistenceLevel) -> Self {
        match cli_level {
            CliPersistenceLevel::Basic => PersistenceLevel::Basic,
            CliPersistenceLevel::Enhanced => PersistenceLevel::Enhanced,
            CliPersistenceLevel::High => PersistenceLevel::High,
            CliPersistenceLevel::Maximum => PersistenceLevel::Maximum,
        }
    }
}

/// Main CLI arguments for GalleonFS
#[derive(Parser)]
#[command(name = "galleonfs")]
#[command(about = "A distributed, high-performance, network-replicated filesystem")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct MainCli {
    /// Run in daemon mode (persistent background service)
    #[arg(short, long)]
    daemon: bool,

    /// Storage path for the node
    #[arg(long, default_value = "./galleonfs_storage")]
    storage_path: PathBuf,

    /// Block size for storage operations
    #[arg(long, default_value = "4096")]
    block_size: u64,

    /// Address to bind the storage service to
    #[arg(long, default_value = "127.0.0.1:8080")]
    bind_address: String,

    /// Address to bind the IPC service to (daemon mode only)
    #[arg(long, default_value = "127.0.0.1:8090")]
    ipc_address: String,

    /// Peer addresses for cluster formation
    #[arg(long, value_delimiter = ',')]
    peer_addresses: Vec<String>,

    /// Replication strategy
    #[arg(long, default_value = "synchronous")]
    replication_strategy: CliReplicationStrategy,

    /// Persistence level
    #[arg(long, default_value = "enhanced")]
    persistence_level: CliPersistenceLevel,

    /// Mount point for volumes (daemon mode only)
    #[arg(long)]
    mount_point: Option<PathBuf>,

    /// Legacy demo mode support
    #[arg(long)]
    demo_mode: bool,

    /// Legacy setup defaults support
    #[arg(long)]
    setup_defaults: bool,

    /// Legacy test replication support
    #[arg(long)]
    test_replication: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    let args: Vec<String> = std::env::args().collect();
    
    // Check if this is a daemon invocation (has -d or --daemon)
    let is_daemon = args.iter().any(|arg| arg == "-d" || arg == "--daemon");
    
    // Check if this is a legacy mode invocation
    let is_legacy = args.iter().any(|arg| {
        arg == "--demo-mode" || arg == "--setup-defaults" || arg == "--test-replication"
    });
    
    // Check if this is a CLI subcommand invocation
    let is_cli_subcommand = has_subcommand(&args);

    if is_daemon || is_legacy {
        // Parse with MainCli for daemon/legacy modes
        let main_cli = MainCli::parse();
        
        // Handle legacy modes first
        if main_cli.demo_mode || main_cli.setup_defaults || main_cli.test_replication {
            return run_legacy_mode(main_cli).await;
        }

        if main_cli.daemon {
            // Run in daemon mode
            return run_daemon_mode(main_cli).await;
        }
    } else if is_cli_subcommand {
        // Parse with GalleonCli for CLI subcommands
        let galleon_cli = GalleonCli::try_parse_from(&args)?;
        let client = GalleonClient::new(galleon_cli.daemon_address.clone());
        return client.execute(galleon_cli).await;
    } else {
        // No subcommands or daemon flag, show help
        eprintln!("GalleonFS - Distributed Filesystem");
        eprintln!("");
        eprintln!("USAGE:");
        eprintln!("    galleonfs -d [OPTIONS]         # Run daemon mode");
        eprintln!("    galleonfs volume <COMMAND>     # Volume management");
        eprintln!("    galleonfs cluster <COMMAND>    # Cluster management");
        eprintln!("    galleonfs daemon <COMMAND>     # Daemon management");
        eprintln!("");
        eprintln!("For more help, try:");
        eprintln!("    galleonfs -h                   # Show all options");
        eprintln!("    galleonfs volume -h            # Volume commands");
        eprintln!("    galleonfs cluster -h           # Cluster commands");
        eprintln!("    galleonfs daemon -h            # Daemon commands");
        return Ok(());
    }

    Ok(())
}

/// Check if the command line arguments contain subcommands
fn has_subcommand(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(arg.as_str(), "volume" | "cluster" | "daemon" | "storage-class")
    })
}

/// Run GalleonFS in daemon mode
async fn run_daemon_mode(main_cli: MainCli) -> Result<()> {
    info!("Starting GalleonFS in daemon mode...");
    
    let config = DaemonConfig {
        storage_path: main_cli.storage_path,
        block_size: main_cli.block_size,
        bind_address: main_cli.bind_address,
        ipc_address: main_cli.ipc_address,
        peer_addresses: main_cli.peer_addresses.clone(),
        replication_strategy: main_cli.replication_strategy.into(),
        persistence_level: main_cli.persistence_level.into(),
        mount_point: main_cli.mount_point,
    };

    info!("Daemon configuration:");
    info!("  Storage path: {:?}", config.storage_path);
    info!("  Block size: {} bytes", config.block_size);
    info!("  Bind address: {}", config.bind_address);
    info!("  IPC address: {}", config.ipc_address);
    info!("  Replication strategy: {:?}", config.replication_strategy);
    info!("  Persistence level: {:?}", config.persistence_level);
    info!("  Peer addresses: {:?}", config.peer_addresses);

    // Initialize cluster manager
    let capabilities = get_default_node_capabilities();
    let mut cluster_manager = ClusterManager::new(config.bind_address.clone(), capabilities);

    // Initialize or join cluster
    if main_cli.peer_addresses.is_empty() {
        info!("No peers specified, initializing new cluster");
        cluster_manager.initialize_cluster("galleonfs-cluster".to_string()).await?;
    } else {
        info!("Peers specified, attempting to join existing cluster");
        let peer_address = main_cli.peer_addresses[0].clone();
        cluster_manager.join_cluster(peer_address).await?;
    }

    // Start cluster background tasks
    cluster_manager.start_background_tasks().await?;

    // Create and start daemon
    let daemon = GalleonDaemon::new(config)?;
    daemon.start().await?;

    Ok(())
}


/// Run legacy modes for backward compatibility
async fn run_legacy_mode(main_cli: MainCli) -> Result<()> {
    use galleonfs::{
        GalleonFS,
        VolumeType,
        WriteConcern,
        StorageClass,
        ReclaimPolicy,
        VolumeBindingMode,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    info!("Running in legacy compatibility mode");
    
    let storage_engine = Arc::new(FileStorageEngine::new(
        main_cli.storage_path.clone(),
        main_cli.block_size,
    ));

    let galleonfs = GalleonFS::new(
        storage_engine,
        main_cli.replication_strategy.into(),
        main_cli.persistence_level.into(),
        main_cli.peer_addresses.clone(),
    );

    if main_cli.setup_defaults {
        info!("Setting up default storage classes and policies...");
        setup_default_configuration(&galleonfs).await?;
        info!("Default configuration completed");
    }

    if main_cli.demo_mode {
        info!("Running in demo mode");
        return run_demo(&galleonfs).await;
    }

    if main_cli.test_replication {
        info!("Running replication and double mount test");
        return galleonfs::replication_mount_test::run_replication_and_double_mount_test().await;
    }

    // Legacy mount mode
    if let Some(base_mount_point) = &main_cli.mount_point {
        info!("Starting GalleonFS with volume mounting at: {:?}", base_mount_point);
        
        std::fs::create_dir_all(base_mount_point)?;
        let mount_manager = galleonfs.create_mount_manager();
        
        let galleonfs_clone = galleonfs.clone();
        let bind_address_clone = main_cli.bind_address.clone();
        tokio::spawn(async move {
            if let Err(e) = galleonfs_clone.run(bind_address_clone).await {
                tracing::error!("Replication service error: {}", e);
            }
        });
        
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        info!("GalleonFS mount service started. Volumes can now be mounted individually.");
        info!("Example: each volume will be mountable at {}/volume-name", base_mount_point.display());
        
        return run_mount_demo(&galleonfs, &mount_manager, base_mount_point).await;
    } else {
        info!("Starting GalleonFS service...");
        galleonfs.run(main_cli.bind_address).await
    }
}

/// Setup default storage classes (legacy compatibility)
async fn setup_default_configuration(galleonfs: &galleonfs::GalleonFS) -> Result<()> {
    use galleonfs::{StorageClass, ReclaimPolicy, VolumeBindingMode};
    use std::collections::HashMap;

    let fast_local_ssd = StorageClass {
        name: "fast-local-ssd".to_string(),
        provisioner: "galleonfs.io/local-ssd".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("disk_type".to_string(), "ssd".to_string());
            params.insert("fs_type".to_string(), "ext4".to_string());
            params.insert("encryption".to_string(), "false".to_string());
            params
        },
        reclaim_policy: ReclaimPolicy::Delete,
        volume_binding_mode: VolumeBindingMode::Immediate,
        allowed_topologies: vec!["zone-a".to_string(), "zone-b".to_string()],
        mount_options: vec!["noatime".to_string(), "nodiratime".to_string()],
    };

    let encrypted_storage = StorageClass {
        name: "encrypted-storage".to_string(),
        provisioner: "galleonfs.io/distributed".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("encryption".to_string(), "true".to_string());
            params.insert("encryption_algorithm".to_string(), "AES-256-GCM".to_string());
            params.insert("key_management".to_string(), "internal-kms".to_string());
            params.insert("replication".to_string(), "3".to_string());
            params
        },
        reclaim_policy: ReclaimPolicy::Retain,
        volume_binding_mode: VolumeBindingMode::WaitForFirstConsumer,
        allowed_topologies: vec!["zone-a".to_string(), "zone-b".to_string(), "zone-c".to_string()],
        mount_options: vec!["noatime".to_string()],
    };

    galleonfs.create_storage_class(fast_local_ssd).await?;
    galleonfs.create_storage_class(encrypted_storage).await?;

    info!("Created default storage classes: fast-local-ssd, encrypted-storage");
    Ok(())
}

/// Legacy demo mode implementation
async fn run_demo(galleonfs: &galleonfs::GalleonFS) -> Result<()> {
    use galleonfs::{VolumeType, WriteConcern};

    info!("=== GalleonFS Production Demo ===");

    // Demo 1: Storage Classes
    info!("1. Demonstrating Storage Classes");
    setup_default_configuration(galleonfs).await?;
    
    let storage_classes = galleonfs.list_storage_classes().await?;
    info!("Available storage classes: {:?}", storage_classes.iter().map(|sc| &sc.name).collect::<Vec<_>>());

    // Demo 2: Volume Creation with Different Classes
    info!("2. Creating volumes with different storage classes");
    
    let fast_volume = galleonfs
        .create_volume(VolumeType::Persistent, 512 * 1024 * 1024, "fast-local-ssd".to_string()) // 512MB
        .await?;
    info!("Created fast SSD volume: {:?}", fast_volume.id);

    let encrypted_volume = galleonfs
        .create_volume(VolumeType::Persistent, 1024 * 1024 * 1024, "encrypted-storage".to_string()) // 1GB
        .await?;
    info!("Created encrypted volume: {:?}", encrypted_volume.id);

    // Demo 3: Writing data with different write concerns
    info!("3. Demonstrating different write concerns");
    
    let demo_data_1 = b"Fast SSD data - optimized for performance";
    let demo_data_2 = b"Encrypted data - secured with AES-256-GCM";
    
    info!("Writing to fast SSD volume with WriteDurable concern...");
    galleonfs
        .write_block(fast_volume.id, 0, demo_data_1, WriteConcern::WriteDurable)
        .await?;

    info!("Writing to encrypted volume with WriteReplicated concern...");
    galleonfs
        .write_block(encrypted_volume.id, 0, demo_data_2, WriteConcern::WriteReplicated)
        .await?;

    info!("=== Demo completed successfully! ===");
    Ok(())
}

/// Legacy mount demo implementation
async fn run_mount_demo(
    galleonfs: &galleonfs::GalleonFS,
    mount_manager: &galleonfs::volume_mount::VolumeMountManager,
    base_mount_point: &std::path::Path
) -> Result<()> {
    use galleonfs::VolumeType;

    info!("=== GalleonFS Volume Mounting Demo ===");

    setup_default_configuration(galleonfs).await?;

    info!("1. Creating test volumes");
    
    let web_volume = galleonfs
        .create_volume(VolumeType::Persistent, 100 * 1024 * 1024, "fast-local-ssd".to_string()) // 100MB
        .await?;
    info!("Created web server volume: {:?}", web_volume.id);

    let db_volume = galleonfs
        .create_volume(VolumeType::Persistent, 500 * 1024 * 1024, "encrypted-storage".to_string()) // 500MB
        .await?;
    info!("Created database volume: {:?}", db_volume.id);

    info!("2. Mounting volumes at individual paths");

    let web_mount_point = base_mount_point.join("web-server");
    let db_mount_point = base_mount_point.join("database");

    let web_mount_id = mount_manager.mount_volume(
        web_volume.id,
        web_mount_point.clone(),
        vec!["rw".to_string(), "sync".to_string()]
    ).await?;
    info!("Web volume mounted at: {}", web_mount_point.display());

    let db_mount_id = mount_manager.mount_volume(
        db_volume.id,
        db_mount_point.clone(),
        vec!["rw".to_string(), "encrypted".to_string()]
    ).await?;
    info!("Database volume mounted at: {}", db_mount_point.display());

    // Cleanup
    mount_manager.unmount_volume(web_mount_id).await?;
    mount_manager.unmount_volume(db_mount_id).await?;

    info!("=== Volume Mounting Demo Completed! ===");
    Ok(())
}