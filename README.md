![Logo](branding\logo-wide-transparent.svg)

A high-performance, distributed, network-replicated storage system built in Rust. GalleonFS provides enterprise-grade volume management with advanced features like snapshots, backup/recovery, migration, and comprehensive monitoring.

## Features

### Core Storage Engine
- **Block-level storage** with configurable block sizes
- **Distributed replication** with synchronous and asynchronous modes  
- **Write-ahead journaling** for crash consistency and recovery
- **Checksum verification** for data integrity
- **Multiple storage classes** with different performance profiles

### Volume Management
- **Dynamic volume provisioning** with storage class selection
- **Volume lifecycle management** (create, expand, clone, delete)
- **Multiple access modes** (ReadWriteOnce, ReadOnlyMany, ReadWriteMany)
- **Volume snapshots** with point-in-time recovery
- **Online volume expansion** and cloning

### Backup & Recovery  
- **Multiple backup strategies** (Full, Incremental, Differential)
- **Automated backup scheduling** with retention policies
- **Point-in-time recovery** from backups or snapshots
- **Application-consistent backups** with pre/post hooks
- **Cross-region backup support** for disaster recovery

### Performance & Monitoring
- **Real-time metrics** (IOPS, throughput, latency, queue depth)
- **Quality of Service (QoS)** policies with limits and guarantees
- **Burstable performance** for handling traffic spikes
- **Performance profiling** and bottleneck identification
- **Cache hit rate tracking** and optimization

### Security & Encryption
- **Encryption at rest** with AES-256-GCM
- **Multiple key management** options (Internal, External, HSM)
- **Access control policies** with fine-grained permissions
- **Audit logging** for all storage operations
- **Multi-tenant isolation** with namespace protection

### Advanced Features
- **Live migration** between storage classes and nodes
- **Storage pools** for resource grouping and isolation
- **Custom storage drivers** with extensible plugin architecture
- **Geo-replication** for multi-region deployments
- **Comprehensive monitoring** with Prometheus integration

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/your-org/galleonfs.git
cd galleonfs

# Build the project
cargo build --release

# Run with default configuration
./target/release/galleonfs
```

### Basic Usage

```bash
# Start GalleonFS with demo mode to see all features
./target/release/galleonfs --demo-mode --setup-defaults

# Start with custom configuration
./target/release/galleonfs \
  --replication-strategy synchronous \
  --persistence-level high \
  --storage-path /var/lib/galleonfs \
  --bind-address 0.0.0.0:8080
```

## Architecture

GalleonFS uses a layered architecture with the following components:

```
┌─────────────────────────────────────────────────────────┐
│                    Application Layer                    │
├─────────────────────────────────────────────────────────┤
│                  GalleonFS API Layer                    │
├─────────────────────────────────────────────────────────┤
│  Volume Mgmt │  Backup Ctrl │  Migration │  Monitoring  │
├─────────────────────────────────────────────────────────┤
│             Storage Engine & Replication                │
├─────────────────────────────────────────────────────────┤
│        Physical Storage (Local, Network, Cloud)         │
└─────────────────────────────────────────────────────────┘
```

### Core Components

- **Storage Engine**: Handles block-level operations, journaling, and consistency
- **Volume Manager**: Manages volume lifecycle and metadata
- **Replication Service**: Provides data redundancy across nodes
- **Backup Controller**: Orchestrates backup and recovery operations
- **Migration Service**: Handles live volume migrations
- **QoS Manager**: Enforces performance policies and resource limits

## Configuration

### Storage Classes

GalleonFS supports multiple storage classes for different performance requirements:

```rust
// Fast local SSD storage
let fast_ssd = StorageClass {
    name: "fast-local-ssd".to_string(),
    provisioner: "galleonfs.io/local-ssd".to_string(),
    parameters: {
        "disk_type" => "ssd",
        "fs_type" => "ext4", 
        "encryption" => "false"
    },
    reclaim_policy: ReclaimPolicy::Delete,
    volume_binding_mode: VolumeBindingMode::Immediate,
};

// Encrypted distributed storage
let encrypted_storage = StorageClass {
    name: "encrypted-storage".to_string(), 
    provisioner: "galleonfs.io/distributed".to_string(),
    parameters: {
        "encryption" => "true",
        "encryption_algorithm" => "AES-256-GCM",
        "replication" => "3"
    },
    reclaim_policy: ReclaimPolicy::Retain,
    volume_binding_mode: VolumeBindingMode::WaitForFirstConsumer,
};
```

### Backup Policies

Configure automated backup schedules with retention:

```rust
let backup_policy = BackupPolicy {
    name: "daily-incremental".to_string(),
    schedule: "0 2 * * *".to_string(), // Daily at 2 AM
    retention: BackupRetention {
        daily: 7,
        weekly: 4, 
        monthly: 3
    },
    strategy: BackupStrategy::Incremental,
    consistency: ConsistencyLevel::Application,
};
```

### QoS Policies

Define performance limits and guarantees:

```rust
let qos_policy = QoSPolicy {
    name: "database-qos".to_string(),
    limits: QoSLimits {
        iops: Some(1000),
        throughput_mbps: Some(100),
        latency_ms: Some(10)
    },
    guarantees: QoSGuarantees {
        iops: Some(500),
        throughput_mbps: Some(50)
    },
    burstable: Some(BurstableQoS {
        enabled: true,
        duration_seconds: 300, // 5 minutes
        iops_multiplier: 2.0,
        throughput_multiplier: 2.0
    })
};
```

## Volume Mounting

GalleonFS provides a production-ready volume mounting system that allows each volume to be individually mounted at separate paths, enabling applications to access volume data through standard filesystem operations.

### Individual Volume Mounting

Each volume can be mounted at its own path:

```bash
# Start GalleonFS with mount capabilities
./galleonfs --mount-point /mnt/galleonfs

# Each volume appears as a separate mount point
/mnt/galleonfs/
├── web-server/          # Individual volume mount point
│   ├── data            # Volume data file
│   ├── README.txt      # Volume information
│   └── .galleonfs_metadata
├── database/           # Another volume mount point
│   ├── data
│   ├── README.txt
│   └── .galleonfs_metadata
└── cache/              # Third volume mount point
    ├── data
    ├── README.txt
    └── .galleonfs_metadata
```

### Volume Operations Through Filesystem

Once mounted, applications can use standard filesystem operations:

```bash
# Write data to a volume
echo "Hello GalleonFS!" > /mnt/galleonfs/web-server/data

# Read data from a volume  
cat /mnt/galleonfs/database/data

# Copy files to volumes
cp myapp.conf /mnt/galleonfs/web-server/data

# Stream large data
dd if=large-file.bin of=/mnt/galleonfs/database/data bs=1M
```

### Programmatic Volume Access

```rust
use galleonfs::volume_mount::{VolumeMountManager, GalleonVolumeFile};
use std::path::PathBuf;

// Create mount manager
let mount_manager = galleonfs.create_mount_manager();

// Mount a volume at a specific path
let mount_id = mount_manager.mount_volume(
    volume_id,
    PathBuf::from("/app/data"),
    vec!["rw".to_string(), "sync".to_string()]
).await?;

// Open volume file for operations
let mut volume_file = mount_manager
    .open_volume_file(&PathBuf::from("/app/data"))
    .await?;

// Read/write data directly
let mut buffer = vec![0; 1024];
let bytes_read = volume_file.read(&mut buffer).await?;
volume_file.write(b"application data").await?;
volume_file.seek(std::io::SeekFrom::Start(0))?;

// Unmount when done
mount_manager.unmount_volume(mount_id).await?;
```

### Multiple Concurrent Mounts

The same volume can be mounted multiple times for shared access:

```rust
// Mount volume for primary application (read-write)
let primary_mount = mount_manager.mount_volume(
    volume_id,
    PathBuf::from("/app/primary"),
    vec!["rw".to_string()]
).await?;

// Mount same volume read-only for monitoring
let readonly_mount = mount_manager.mount_volume(
    volume_id,
    PathBuf::from("/app/readonly"), 
    vec!["ro".to_string()]
).await?;

// List all mounts
let active_mounts = mount_manager.list_mounts().await;
for mount in active_mounts {
    println!("Volume {} mounted at {}", 
             mount.volume_id, mount.mount_point.display());
}
```

## API Examples

### Volume Operations

```rust
use galleonfs::{GalleonFS, VolumeType, WriteConcern};

// Create a new volume
let volume = galleonfs.create_volume(
    VolumeType::Persistent,
    1024 * 1024 * 1024, // 1GB
    "fast-local-ssd".to_string()
).await?;

// Write data with durability guarantee  
galleonfs.write_block(
    volume.id,
    0, // block_id
    b"Hello, GalleonFS!",
    WriteConcern::WriteDurable
).await?;

// Read data back
let data = galleonfs.read_block(volume.id, 0).await?;

// Create a snapshot
let snapshot = galleonfs.create_snapshot(
    volume.id,
    "backup-before-upgrade"
).await?;

// Clone the volume
let cloned_volume = galleonfs.clone_volume(
    volume.id,
    "development-copy"  
).await?;
```

### Performance Monitoring

```rust
// Get real-time metrics
let metrics = galleonfs.get_volume_metrics(volume.id).await?;
println!("IOPS: {:.2}, Throughput: {:.2} MB/s, Latency: {:.2} ms",
    metrics.iops, metrics.throughput_mbps, metrics.latency_ms);

// Check volume usage
let usage = galleonfs.get_volume_usage(volume.id).await?;
println!("Volume usage: {} bytes", usage);

// List all volumes
let volumes = galleonfs.list_volumes().await?;
for vol in volumes {
    println!("Volume: {} ({}), State: {:?}, Size: {} bytes",
        vol.name, vol.id, vol.state, vol.size_bytes);
}
```

## Performance Characteristics

| Storage Class | IOPS | Throughput | Latency | Use Case |
|---------------|------|------------|---------|----------|
| local-ssd | High (>10K) | High (>500MB/s) | Low (<1ms) | Databases, High-perf apps |
| local-hdd | Medium (1K-5K) | Medium (100-200MB/s) | Medium (5-10ms) | General storage |
| distributed | Medium (5K-15K) | High (>1GB/s) | Medium (2-5ms) | Replicated workloads |
| encrypted | Medium (3K-8K) | Medium (200-400MB/s) | Medium (3-8ms) | Secure storage |

## Write Concerns & Durability

GalleonFS provides different write concern levels for balancing performance vs durability:

- **WriteAcknowledged**: Fastest, acknowledged after primary write
- **WriteDurable**: Medium, acknowledged after flush to disk
- **WriteReplicated**: Slower, acknowledged after replication to 1+ nodes  
- **WriteDistributed**: Slowest, acknowledged after cross-zone replication

## Replication & High Availability

### Replication Strategies

- **Synchronous**: Real-time replication with strong consistency
- **Asynchronous**: Background replication with eventual consistency

### Persistence Levels

- **Basic**: Local storage, protects against process failures
- **Enhanced**: Local RAID, protects against disk failures
- **High**: Distributed, protects against node failures
- **Maximum**: Geo-replicated, protects against datacenter failures

## Development

### Building from Source

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/OmniCloudOrg/galleonfs
cd galleonfs
cargo build --release

# Run tests
cargo test

# Generate documentation
cargo doc --open
```

### Project Structure

```
galleonfs/
├── src/
│   ├── lib.rs           # Core types and GalleonFS struct
│   ├── storage.rs       # Storage engine implementation
│   ├── volume.rs        # Volume management  
│   ├── volume_mount.rs  # Volume mounting system
│   ├── snapshot.rs      # Snapshot operations
│   ├── backup.rs        # Backup and recovery
│   ├── migration.rs     # Volume migration
│   ├── replication.rs   # Data replication
│   ├── monitoring.rs    # Performance monitoring
│   ├── security.rs      # Encryption and access control
│   ├── qos.rs          # Quality of Service policies
│   ├── fuse_fs.rs      # FUSE filesystem (Linux/macOS)
│   ├── virtual_fs.rs   # Virtual filesystem (Windows)
│   └── main.rs         # CLI application
├── tests/
│   └── integration_tests.rs
├── Cargo.toml
└── README.md
```

### Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes and add tests
4. Ensure all tests pass (`cargo test`)
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

## Testing

GalleonFS includes comprehensive integration tests:

```bash
# Run all tests
cargo test

# Run specific test module  
cargo test --test integration_tests

# Run tests with output
cargo test -- --nocapture

# Run demo mode for manual testing
cargo run -- --demo-mode --setup-defaults
```

## Monitoring & Observability

### Metrics Integration

GalleonFS exposes metrics compatible with Prometheus:

- Volume-level metrics (IOPS, throughput, latency)
- System-level metrics (capacity, errors, performance)
- Replication metrics (lag, throughput, errors)
- Backup metrics (success rate, duration, size)

### Logging

Structured logging with configurable levels:

```bash
# Set log level
RUST_LOG=info ./target/release/galleonfs

# Debug logging for troubleshooting  
RUST_LOG=debug ./target/release/galleonfs
```

## Deployment

### Single Node

```bash
# Basic single-node setup
./galleonfs --storage-path /var/lib/galleonfs --bind-address 127.0.0.1:8080
```

### Distributed Cluster

```bash
# Node 1 (primary)
./galleonfs --bind-address 10.0.1.10:8080 --peer-addresses 10.0.1.11:8080,10.0.1.12:8080

# Node 2 
./galleonfs --bind-address 10.0.1.11:8080 --peer-addresses 10.0.1.10:8080,10.0.1.12:8080

# Node 3
./galleonfs --bind-address 10.0.1.12:8080 --peer-addresses 10.0.1.10:8080,10.0.1.11:8080  
```

## Roadmap

### Current Features (v0.1)
- ✅ Core storage engine with journaling
- ✅ Volume management and snapshots  
- ✅ Backup/recovery system
- ✅ Performance monitoring
- ✅ Basic replication
- ✅ Encryption at rest

### Planned Features (v0.2)
- 🔄 REST API for external integration
- 🔄 Web-based management interface  
- 🔄 Advanced QoS policies
- 🔄 Compression and deduplication
- 🔄 S3-compatible object storage
- 🔄 Kubernetes CSI driver

### Future Features (v1.0)
- 📋 Multi-datacenter replication
- 📋 Advanced analytics and ML-based optimization
- 📋 Disaster recovery automation
- 📋 Fine-grained access controls
- 📋 Enterprise directory integration
- 📋 Compliance reporting tools

## License

Licensed under the MIT License. See [LICENSE](LICENSE) for details.

**GalleonFS** - Built for performance, designed for scale, engineered for reliability.