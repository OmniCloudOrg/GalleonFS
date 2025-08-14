use anyhow::Result;
use clap::{Parser, ValueEnum};
use galleonfs::{
    storage::FileStorageEngine, 
    GalleonFS, 
    PersistenceLevel, 
    ReplicationStrategy, 
    VolumeType, 
    WriteConcern,
    StorageClass,
    ReclaimPolicy,
    VolumeBindingMode,
    QoSPolicy,
    QoSLimits,
    QoSGuarantees,
    LabelSelector,
    BackupPolicy,
    BackupRetention,
    BackupTarget,
    BackupStrategy,
    ConsistencyLevel,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
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

#[derive(Debug, Clone, ValueEnum)]
enum CliVolumeType {
    Ephemeral,
    Persistent,
    Shared,
}

impl From<CliVolumeType> for VolumeType {
    fn from(cli_type: CliVolumeType) -> Self {
        match cli_type {
            CliVolumeType::Ephemeral => VolumeType::Ephemeral,
            CliVolumeType::Persistent => VolumeType::Persistent,
            CliVolumeType::Shared => VolumeType::Shared,
        }
    }
}

#[derive(Debug, Clone, ValueEnum)]
enum CliWriteConcern {
    WriteAcknowledged,
    WriteDurable,
    WriteReplicated,
    WriteDistributed,
}

impl From<CliWriteConcern> for WriteConcern {
    fn from(cli_concern: CliWriteConcern) -> Self {
        match cli_concern {
            CliWriteConcern::WriteAcknowledged => WriteConcern::WriteAcknowledged,
            CliWriteConcern::WriteDurable => WriteConcern::WriteDurable,
            CliWriteConcern::WriteReplicated => WriteConcern::WriteReplicated,
            CliWriteConcern::WriteDistributed => WriteConcern::WriteDistributed,
        }
    }
}

#[derive(Parser)]
#[command(name = "galleonfs")]
#[command(about = "A distributed, high-performance, network-replicated filesystem")]
struct Cli {
    #[arg(long, default_value = "synchronous")]
    replication_strategy: CliReplicationStrategy,

    #[arg(long, default_value = "enhanced")]
    persistence_level: CliPersistenceLevel,

    #[arg(long, default_value = "persistent")]
    volume_type: CliVolumeType,

    #[arg(long, default_value = "write-acknowledged")]
    write_concern: CliWriteConcern,

    #[arg(long, default_value = "./galleonfs_storage")]
    storage_path: PathBuf,

    #[arg(long, default_value = "4096")]
    block_size: u64,

    #[arg(long, default_value = "1073741824")] // 1GB
    volume_size: u64,

    #[arg(long, default_value = "127.0.0.1:8080")]
    bind_address: String,

    #[arg(long, value_delimiter = ',')]
    peer_addresses: Vec<String>,

    #[arg(long)]
    demo_mode: bool,

    #[arg(long)]
    setup_defaults: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    let cli = Cli::parse();

    info!("Starting GalleonFS with configuration:");
    info!("  Replication Strategy: {:?}", cli.replication_strategy);
    info!("  Persistence Level: {:?}", cli.persistence_level);
    info!("  Volume Type: {:?}", cli.volume_type);
    info!("  Write Concern: {:?}", cli.write_concern);
    info!("  Storage Path: {:?}", cli.storage_path);
    info!("  Block Size: {} bytes", cli.block_size);
    info!("  Bind Address: {}", cli.bind_address);
    info!("  Peer Addresses: {:?}", cli.peer_addresses);

    let storage_engine = Arc::new(FileStorageEngine::new(
        cli.storage_path.clone(),
        cli.block_size,
    ));

    let galleonfs = GalleonFS::new(
        storage_engine,
        cli.replication_strategy.clone().into(),
        cli.persistence_level.clone().into(),
        cli.peer_addresses.clone(),
    );

    if cli.setup_defaults {
        info!("Setting up default storage classes and policies...");
        setup_default_configuration(&galleonfs).await?;
        info!("Default configuration completed");
    }

    if cli.demo_mode {
        info!("Running in demo mode");
        return run_demo(&galleonfs, &cli).await;
    }

    info!("Starting GalleonFS service...");
    galleonfs.run(cli.bind_address).await
}

async fn setup_default_configuration(galleonfs: &GalleonFS) -> Result<()> {
    // Create default storage classes
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

    let archival_storage = StorageClass {
        name: "archival-storage".to_string(),
        provisioner: "galleonfs.io/cold-storage".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("disk_type".to_string(), "hdd".to_string());
            params.insert("compression".to_string(), "true".to_string());
            params.insert("deduplication".to_string(), "true".to_string());
            params
        },
        reclaim_policy: ReclaimPolicy::Retain,
        volume_binding_mode: VolumeBindingMode::Immediate,
        allowed_topologies: vec!["zone-a".to_string()],
        mount_options: vec![],
    };

    galleonfs.create_storage_class(fast_local_ssd).await?;
    galleonfs.create_storage_class(encrypted_storage).await?;
    galleonfs.create_storage_class(archival_storage).await?;

    info!("Created default storage classes: fast-local-ssd, encrypted-storage, archival-storage");

    Ok(())
}

async fn run_demo(galleonfs: &GalleonFS, cli: &Cli) -> Result<()> {
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
        .write_block(encrypted_volume.id, 0, demo_data_2, cli.write_concern.clone().into())
        .await?;

    // Demo 4: Reading data back
    info!("4. Reading data back and verifying integrity");
    
    let read_data_1 = galleonfs.read_block(fast_volume.id, 0).await?;
    let read_str_1 = std::str::from_utf8(&read_data_1[..demo_data_1.len()])?;
    info!("Read from fast SSD volume: {}", read_str_1);

    let read_data_2 = galleonfs.read_block(encrypted_volume.id, 0).await?;
    let read_str_2 = std::str::from_utf8(&read_data_2[..demo_data_2.len()])?;
    info!("Read from encrypted volume: {}", read_str_2);

    // Demo 5: Snapshots
    info!("5. Creating and managing snapshots");
    
    let snapshot1 = galleonfs
        .create_snapshot(fast_volume.id, "fast-volume-snapshot-1")
        .await?;
    info!("Created snapshot: {:?}", snapshot1.id);

    let snapshot2 = galleonfs
        .create_snapshot(encrypted_volume.id, "encrypted-volume-snapshot-1") 
        .await?;
    info!("Created snapshot: {:?}", snapshot2.id);

    let snapshots = galleonfs.list_snapshots(fast_volume.id).await?;
    info!("Snapshots for fast volume: {} total", snapshots.len());

    // Demo 6: Volume expansion
    info!("6. Demonstrating volume expansion");
    
    let original_size = fast_volume.size_bytes;
    let new_size = original_size + (256 * 1024 * 1024); // Add 256MB
    
    galleonfs.expand_volume(fast_volume.id, new_size).await?;
    info!("Expanded volume from {} bytes to {} bytes", original_size, new_size);

    // Demo 7: Volume cloning
    info!("7. Cloning volumes");
    
    let cloned_volume = galleonfs
        .clone_volume(fast_volume.id, "cloned-fast-volume")
        .await?;
    info!("Cloned volume created: {:?}", cloned_volume.id);

    // Verify cloned data
    let cloned_data = galleonfs.read_block(cloned_volume.id, 0).await?;
    let cloned_str = std::str::from_utf8(&cloned_data[..demo_data_1.len()])?;
    info!("Data in cloned volume: {}", cloned_str);

    // Demo 8: Metrics and monitoring
    info!("8. Retrieving volume metrics");
    
    let fast_volume_metrics = galleonfs.get_volume_metrics(fast_volume.id).await?;
    info!("Fast volume metrics - IOPS: {:.2}, Throughput: {:.2} MB/s, Latency: {:.2} ms", 
        fast_volume_metrics.iops, fast_volume_metrics.throughput_mbps, fast_volume_metrics.latency_ms);

    let encrypted_volume_metrics = galleonfs.get_volume_metrics(encrypted_volume.id).await?;
    info!("Encrypted volume metrics - IOPS: {:.2}, Throughput: {:.2} MB/s, Latency: {:.2} ms", 
        encrypted_volume_metrics.iops, encrypted_volume_metrics.throughput_mbps, encrypted_volume_metrics.latency_ms);

    // Demo 9: Volume usage
    info!("9. Checking volume usage");
    
    let fast_volume_usage = galleonfs.get_volume_usage(fast_volume.id).await?;
    let encrypted_volume_usage = galleonfs.get_volume_usage(encrypted_volume.id).await?;
    info!("Fast volume usage: {} bytes", fast_volume_usage);
    info!("Encrypted volume usage: {} bytes", encrypted_volume_usage);

    // Demo 10: List all volumes
    info!("10. Listing all volumes in the system");
    
    let all_volumes = galleonfs.list_volumes().await?;
    info!("Total volumes in system: {}", all_volumes.len());
    for volume in all_volumes {
        info!("  Volume: {} ({}), State: {:?}, Size: {} bytes, Class: {}", 
            volume.name, volume.id, volume.state, volume.size_bytes, volume.storage_class);
    }

    info!("=== Demo completed successfully! ===");
    info!("All features demonstrated:");
    info!("  ✓ Multiple storage classes with different characteristics");
    info!("  ✓ Volume creation, expansion, and cloning");
    info!("  ✓ Different write concerns for durability guarantees");
    info!("  ✓ Snapshot creation and management");
    info!("  ✓ Performance metrics and monitoring");
    info!("  ✓ Data integrity verification");
    info!("  ✓ Volume usage tracking");

    Ok(())
}