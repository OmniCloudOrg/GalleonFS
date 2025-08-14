![Logo](/branding/logo-wide-transparent.svg)

A high-performance, distributed, network-replicated storage system built in Rust. GalleonFS provides enterprise-grade volume management with advanced features like snapshots, backup/recovery, migration, comprehensive monitoring, and a modern daemon/CLI architecture for seamless cluster management.

## Architecture Overview

GalleonFS supports two operation modes:

### 🚀 Daemon Mode (`-d`)
- **Persistent Background Service**: Runs storage service continuously
- **IPC Communication**: Provides TCP-based endpoint (default: 127.0.0.1:8090)
- **Auto-Cluster Formation**: Automatically initializes or joins clusters
- **Volume Mounting**: Built-in mount manager for volume access
- **Default Storage Classes**: Sets up production-ready storage configurations

### 💻 CLI Mode
- **Runtime Operations**: Communicates with daemon for live management
- **Volume Management**: Create, list, get, delete, mount, unmount volumes
- **Cluster Operations**: Status, join, leave, peer management
- **Daemon Control**: Status, configuration, graceful shutdown
- **Name-Based Operations**: Support for both UUID and name-based volume operations

## Quick Start

### Starting a Cluster

**Single Node (Development)**:
```bash
# Start daemon with basic configuration
galleonfs -d --storage-path ./galleon_node1 --bind-address 127.0.0.1:8081 --ipc-address 127.0.0.1:8091

# Enable volume mounting
galleonfs -d --storage-path ./galleon_node1 --mount-point ./mounts --bind-address 127.0.0.1:8081
```

**Multi-Node Cluster (Production)**:
```bash
# Node 1 (Creates new cluster)
galleonfs -d \
  --storage-path /var/lib/galleonfs/node1 \
  --bind-address 10.0.1.10:8081 \
  --ipc-address 10.0.1.10:8091 \
  --mount-point /mnt/galleonfs

# Node 2 (Joins existing cluster)  
galleonfs -d \
  --storage-path /var/lib/galleonfs/node2 \
  --bind-address 10.0.1.11:8082 \
  --ipc-address 10.0.1.11:8092 \
  --peer-addresses 10.0.1.10:8081 \
  --mount-point /mnt/galleonfs

# Node 3 (Joins existing cluster)
galleonfs -d \
  --storage-path /var/lib/galleonfs/node3 \
  --bind-address 10.0.1.12:8083 \
  --ipc-address 10.0.1.12:8093 \
  --peer-addresses 10.0.1.10:8081,10.0.1.11:8082 \
  --mount-point /mnt/galleonfs
```

### CLI Operations

**Volume Management**:
```bash
# Create volumes with different storage classes
galleonfs volume create --name web-data --size 1G --storage-class fast-local-ssd
galleonfs volume create --name db-data --size 10G --storage-class encrypted-storage --replicas 3

# List all volumes
galleonfs volume list

# Get volume details (by name or ID)
galleonfs volume get web-data
galleonfs volume get a1b2c3d4-e5f6-7890-abcd-ef1234567890

# Mount volumes (requires daemon with --mount-point)
galleonfs volume mount web-data /app/web-data --options rw,sync
galleonfs volume mount db-data /app/database --options rw,encrypted

# Unmount volumes
galleonfs volume unmount web-data
galleonfs volume unmount /app/database

# Delete volumes  
galleonfs volume delete web-data --force
```

**Cluster Management**:
```bash
# Check cluster status
galleonfs cluster status

# List cluster peers
galleonfs cluster peers

# Join existing cluster (runtime)
galleonfs cluster join 10.0.1.10:8081

# Leave cluster (graceful)
galleonfs cluster leave
galleonfs cluster leave --force  # Force leave
```

**Daemon Management**:
```bash
# Check daemon status
galleonfs daemon status

# View daemon configuration
galleonfs daemon config

# Gracefully stop daemon
galleonfs daemon stop

# Restart daemon
galleonfs daemon restart
```

**Storage Class Management**:
```bash
# List available storage classes
galleonfs storage-class list

# Create storage class from configuration file
galleonfs storage-class create storage-class-config.json
```

## Daemon Configuration

### Command Line Options

| Option | Default | Description |
|--------|---------|-------------|
| `-d, --daemon` | false | Run in daemon mode |
| `--storage-path` | `./galleonfs_storage` | Storage directory path |
| `--block-size` | `4096` | Block size for storage operations |
| `--bind-address` | `127.0.0.1:8080` | Storage service bind address |
| `--ipc-address` | `127.0.0.1:8090` | IPC service bind address |
| `--peer-addresses` | `[]` | Comma-separated peer addresses |
| `--replication-strategy` | `synchronous` | Replication strategy |
| `--persistence-level` | `enhanced` | Data persistence level |
| `--mount-point` | `None` | Volume mount point directory |

### Environment Configuration

```bash
# Set log level
export RUST_LOG=info

# For debugging
export RUST_LOG=debug
```

## CLI Reference

### Volume Commands

```bash
galleonfs volume <COMMAND>

Commands:
  create     Create a new volume
  list       List all volumes  
  get        Get volume details by ID or name
  delete     Delete a volume by ID or name
  mount      Mount a volume to filesystem path
  unmount    Unmount a volume by ID or path
```

**Create Volume Options**:
```bash
galleonfs volume create [OPTIONS]

Options:
  -n, --name <NAME>              Volume name (optional)
  -t, --volume-type <TYPE>       Volume type [default: persistent] [possible values: ephemeral, persistent, shared]
  -s, --size <SIZE>              Size with suffix (K/M/G/T) [default: 1G]
  -c, --storage-class <CLASS>    Storage class [default: encrypted-storage]
  -r, --replicas <COUNT>         Number of replicas (overrides storage class)
```

**Mount Volume Options**:
```bash
galleonfs volume mount <ID> <PATH> [OPTIONS]

Options:
  -o, --options <OPTIONS>        Mount options (comma-separated)
```

### Cluster Commands

```bash
galleonfs cluster <COMMAND>

Commands:
  status     Show cluster status and node information
  join       Join an existing cluster via peer address
  leave      Leave the current cluster
  peers      List all peers in the cluster
```

### Daemon Commands

```bash
galleonfs daemon <COMMAND>

Commands:
  status     Show daemon runtime status
  config     Show daemon configuration
  stop       Stop the daemon gracefully
  restart    Restart the daemon
```

### Storage Class Commands

```bash
galleonfs storage-class <COMMAND>

Commands:
  list       List all available storage classes
  create     Create storage class from configuration file
```

## Built-in Storage Classes

GalleonFS automatically creates default storage classes:

### `fast-local-ssd`
- **Type**: Local SSD storage
- **Performance**: High IOPS, low latency
- **Encryption**: Disabled for maximum performance
- **Reclaim Policy**: Delete
- **Use Case**: High-performance applications, databases

### `encrypted-storage`  
- **Type**: Distributed replicated storage
- **Encryption**: AES-256-GCM
- **Replication**: 3 replicas across zones
- **Reclaim Policy**: Retain
- **Use Case**: Secure data, compliance requirements

## Cluster Management

### Unified Cluster Membership

GalleonFS follows a **Docker Swarm-like** cluster model:

- **Single Cluster Membership**: Nodes belong to exactly one cluster
- **Auto-Discovery**: New nodes automatically discover and join existing clusters
- **Leader Election**: Automatic leader selection for cluster coordination
- **Health Monitoring**: Continuous peer health checking
- **Graceful Handoff**: Clean node removal without data loss

### Volume Placement Strategies

- **Spread**: Distribute across maximum number of nodes
- **Balanced**: Even distribution based on capacity
- **Affinity**: Place volumes near specific resources
- **Anti-Affinity**: Avoid co-location with specific resources

## Legacy Compatibility

GalleonFS maintains backward compatibility with existing deployments:

```bash
# Legacy mount mode (still supported)
galleonfs --mount-point /mnt/galleonfs --bind-address 127.0.0.1:8080

# Legacy demo mode
galleonfs --demo-mode --setup-defaults

# Legacy replication testing
galleonfs --test-replication
```

## Features

### Core Storage Engine
- **Block-level storage** with configurable block sizes
- **Distributed replication** with synchronous and asynchronous modes  
- **Write-ahead journaling** for crash consistency and recovery
- **Checksum verification** for data integrity
- **Multiple storage classes** with different performance profiles

### Modern Architecture
- **Daemon/CLI Architecture**: Persistent service with runtime management
- **IPC Communication**: TCP-based inter-process communication
- **Cluster Management**: Docker Swarm-like unified cluster membership
- **Auto-Discovery**: Automatic peer detection and cluster formation
- **Volume Mounting**: Individual volume mounting with filesystem access

### Volume Management
- **Dynamic volume provisioning** with storage class selection
- **Volume lifecycle management** (create, expand, clone, delete)
- **Multiple access modes** (ReadWriteOnce, ReadOnlyMany, ReadWriteMany)
- **Volume snapshots** with point-in-time recovery
- **Online volume expansion** and cloning
- **Name-based operations** for improved usability

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

## Installation

```bash
# Clone the repository
git clone https://github.com/OmniCloudOrg/GalleonFS.git
cd galleonfs

# Build the project
cargo build --release

# Installation
sudo cp target/release/galleonfs /usr/local/bin/
```

## System Architecture

GalleonFS uses a modern layered architecture:

```
┌─────────────────────────────────────────────────────────┐
│                CLI / External APIs                      │
├─────────────────────────────────────────────────────────┤
│              IPC Communication Layer                    │
├─────────────────────────────────────────────────────────┤
│                   GalleonFS Daemon                      │
├─────────────────────────────────────────────────────────┤
│  Volume Mgmt │  Cluster Mgmt │  Mount Mgmt │ Storage Cls │
├─────────────────────────────────────────────────────────┤
│             Storage Engine & Replication                │
├─────────────────────────────────────────────────────────┤
│        Physical Storage (Local, Network, Cloud)         │
└─────────────────────────────────────────────────────────┘
```

### Core Components

- **CLI Client**: User-facing command-line interface
- **IPC Layer**: TCP-based communication between CLI and daemon
- **GalleonFS Daemon**: Persistent storage service
- **Volume Manager**: Manages volume lifecycle and metadata
- **Cluster Manager**: Handles node membership and coordination
- **Mount Manager**: Provides filesystem access to volumes
- **Storage Engine**: Handles block-level operations, journaling, and consistency
- **Replication Service**: Provides data redundancy across nodes

## Troubleshooting

### Common Issues

**Daemon Connection Failed**:
```bash
# Check if daemon is running
galleonfs daemon status

# Check daemon logs (with debug output)
RUST_LOG=debug galleonfs -d --storage-path ./debug_node

# Verify IPC address is correct
galleonfs --daemon-address 127.0.0.1:8090 daemon status
```

**Mount Operations Fail**:
```bash
# Ensure daemon was started with mount-point
galleonfs -d --mount-point /mnt/galleonfs --storage-path ./node1

# Check mount directory permissions
sudo chown $USER:$USER /mnt/galleonfs
chmod 755 /mnt/galleonfs
```

**Cluster Join Issues**:
```bash
# Verify peer addresses are reachable
ping 10.0.1.10
telnet 10.0.1.10 8081

# Check firewall settings
sudo ufw allow 8081
sudo ufw allow 8090
```

**Volume Operations Timeout**:
```bash
# Check storage path permissions
ls -la ./galleonfs_storage/

# Monitor disk space
df -h

# Check system resources
top
iostat -x 1
```

## Configuration Examples

### Storage Classes

Create custom storage classes for different workloads:

**High-Performance SSD (JSON)**:
```json
{
  "name": "ultra-fast-ssd",
  "provisioner": "galleonfs.io/nvme-ssd",
  "parameters": {
    "disk_type": "nvme",
    "fs_type": "xfs",
    "encryption": "false",
    "raid_level": "1"
  },
  "reclaim_policy": "Delete",
  "volume_binding_mode": "Immediate",
  "allowed_topologies": ["zone-a", "zone-b"],
  "mount_options": ["noatime", "nodiratime", "nobarrier"]
}
```

**Encrypted Distributed Storage (YAML)**:
```yaml
name: secure-distributed
provisioner: galleonfs.io/distributed
parameters:
  encryption: "true"
  encryption_algorithm: "AES-256-GCM"
  key_management: "external-kms"
  replication: "5"
  consistency_level: "strong"
reclaim_policy: Retain
volume_binding_mode: WaitForFirstConsumer
allowed_topologies:
  - zone-a
  - zone-b  
  - zone-c
mount_options:
  - noatime
  - sync
```

### Daemon Configuration File

Create `/etc/galleonfs/daemon.conf`:
```toml
[daemon]
storage_path = "/var/lib/galleonfs"
block_size = 4096
bind_address = "0.0.0.0:8080"
ipc_address = "127.0.0.1:8090"
mount_point = "/mnt/galleonfs"

[cluster]
peer_addresses = ["10.0.1.11:8080", "10.0.1.12:8080"]
auto_join = true
health_check_interval = 30

[replication]
strategy = "synchronous"
min_replicas = 2
max_replicas = 5

[persistence]
level = "high"
journal_size = "256MB"
checkpoint_interval = 300

[security]
encryption_at_rest = true
tls_enabled = false
auth_required = false
```

### Systemd Service

Create `/etc/systemd/system/galleonfs.service`:
```ini
[Unit]
Description=GalleonFS Distributed Storage Daemon
After=network.target
Wants=network.target

[Service]
Type=simple
User=galleonfs
Group=galleonfs
ExecStart=/usr/local/bin/galleonfs -d \
  --storage-path /var/lib/galleonfs \
  --bind-address 0.0.0.0:8080 \
  --ipc-address 127.0.0.1:8090 \
  --mount-point /mnt/galleonfs \
  --peer-addresses 10.0.1.11:8080,10.0.1.12:8080
ExecReload=/bin/kill -HUP $MAINPID
Restart=always
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

Start the service:
```bash
sudo systemctl enable galleonfs
sudo systemctl start galleonfs
sudo systemctl status galleonfs
```

### Storage Classes (Legacy Configuration)

Default storage classes setup in code:

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

# Install system dependencies (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install -y libfuse-dev pkg-config

# Clone and build
git clone https://github.com/OmniCloudOrg/GalleonFS.git
cd galleonfs
cargo build --release

# Run tests
cargo test

# Generate documentation
cargo doc --open
```

### Development Environment

**Start Development Cluster**:
```bash
# Terminal 1: Node 1
cargo run -- -d \
  --storage-path ./dev_node1 \
  --bind-address 127.0.0.1:8081 \
  --ipc-address 127.0.0.1:8091 \
  --mount-point ./dev_mounts1

# Terminal 2: Node 2  
cargo run -- -d \
  --storage-path ./dev_node2 \
  --bind-address 127.0.0.1:8082 \
  --ipc-address 127.0.0.1:8092 \
  --peer-addresses 127.0.0.1:8081 \
  --mount-point ./dev_mounts2

# Terminal 3: CLI operations
cargo run -- volume create --name test-vol --size 100M
cargo run -- volume list
cargo run -- cluster status
```

### Project Structure

```
galleonfs/
├── src/
│   ├── lib.rs           # Core types and GalleonFS struct
│   ├── main.rs          # CLI application entry point
│   ├── daemon.rs        # Daemon service and IPC handling
│   ├── cli.rs           # CLI client and command parsing
│   ├── cluster.rs       # Cluster management and membership
│   ├── storage.rs       # Storage engine implementation
│   ├── volume.rs        # Volume management  
│   ├── volume_mount.rs  # Volume mounting system
│   ├── replication.rs   # Data replication across nodes
│   ├── snapshot.rs      # Snapshot operations
│   ├── backup.rs        # Backup and recovery
│   ├── migration.rs     # Volume migration
│   ├── monitoring.rs    # Performance monitoring
│   ├── security.rs      # Encryption and access control
│   ├── qos.rs          # Quality of Service policies
│   ├── fuse_fs.rs      # FUSE filesystem (Linux/macOS)
│   └── virtual_fs.rs   # Virtual filesystem (Windows)
├── tests/
│   └── integration_tests.rs
├── Cargo.toml
├── README.md
└── .gitignore
```

### Testing

**Unit Tests**:
```bash
# Run all tests
cargo test

# Run specific test module  
cargo test --test integration_tests

# Run tests with output
cargo test -- --nocapture

# Run daemon/CLI integration tests
cargo test daemon_cli_integration
```

**Manual Testing**:
```bash
# Demo mode with all features
cargo run -- --demo-mode --setup-defaults

# Test replication functionality
cargo run -- --test-replication

# Performance testing
cargo run -- -d --storage-path ./perf_test &
for i in {1..100}; do
  cargo run -- volume create --name vol-$i --size 10M
done
```

### Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes and add tests
4. Ensure all tests pass (`cargo test`)
5. Test daemon/CLI functionality manually
6. Commit your changes (`git commit -m 'Add amazing feature'`)
7. Push to the branch (`git push origin feature/amazing-feature`)
8. Open a Pull Request

### Code Style

- Follow Rust standard formatting (`cargo fmt`)
- Add documentation for public APIs
- Include unit tests for new functionality
- Update integration tests for CLI/daemon changes
- Use `tracing` for logging instead of `println!`

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

### ✅ Current Features (v0.2)
- **Modern Architecture**: Daemon/CLI separation with IPC communication
- **Cluster Management**: Docker Swarm-like unified cluster membership  
- **Volume Operations**: Create, list, get, delete with name-based lookup
- **Volume Mounting**: Individual volume mounting with filesystem access
- **Storage Classes**: Built-in and custom storage class support
- **Core Storage**: Block-level storage with journaling and replication
- **Performance Monitoring**: Real-time metrics and monitoring
- **Encryption**: AES-256-GCM encryption at rest
- **Backup System**: Multiple backup strategies and recovery
- **CLI Interface**: Comprehensive command-line management
- **Auto-Discovery**: Automatic peer detection and cluster formation

### 🚧 In Progress (v0.3)
- **Enhanced Cluster Operations**: Dynamic node addition/removal
- **Volume Migration**: Live migration between nodes and storage classes
- **Advanced Mounting**: Unmount by path, multi-mount support
- **Storage Class Templates**: YAML/JSON configuration file support
- **Daemon Restart**: Graceful restart without data loss
- **Health Monitoring**: Advanced peer health checking and recovery

### 📋 Planned Features (v0.4)
- **REST API**: HTTP API for external integration and web UIs
- **Web Management Interface**: Browser-based cluster management
- **Advanced QoS**: Sophisticated performance policies and limits
- **Compression & Deduplication**: Space-efficient storage
- **S3-Compatible API**: Object storage interface
- **RBAC**: Role-based access control and user management

### 🔮 Future Features (v1.0)
- **Multi-Datacenter Replication**: Cross-region data replication
- **AI-Powered Optimization**: ML-based performance and placement optimization  
- **Disaster Recovery Automation**: Automated failover and recovery
- **Advanced Analytics**: Predictive analytics and capacity planning
- **Enterprise Directory Integration**: LDAP/AD authentication
- **Compliance Tools**: SOC2, HIPAA, PCI-DSS compliance reporting
- **Container Orchestrator Integration**: Native Kubernetes CSI driver
- **Edge Computing**: Edge node support with intermittent connectivity

### Migration Guide (v0.1 → v0.2)

**Existing Deployments**:
```bash
# Old single-mode operation (still supported)
galleonfs --mount-point /mnt/galleonfs --bind-address 127.0.0.1:8080

# New daemon mode (recommended)  
galleonfs -d --mount-point /mnt/galleonfs --bind-address 127.0.0.1:8080 --ipc-address 127.0.0.1:8090

# CLI operations (new)
galleonfs volume list
galleonfs cluster status
```

**Breaking Changes**: None - full backward compatibility maintained.

## License

Licensed under the MIT License. See [LICENSE](LICENSE) for details.

**GalleonFS** - Built for performance, designed for scale, engineered for reliability.
