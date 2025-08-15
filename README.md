![Logo](/branding/logo-wide-transparent.svg)

A high-performance, distributed, network-replicated storage system built in Rust. GalleonFS provides enterprise-grade volume management with advanced features like snapshots, backup/recovery, migration, comprehensive monitoring, and a modern daemon/CLI architecture with cross-platform virtual filesystem (VFS) support for seamless cluster management.

## 🆕 Cross-Platform Virtual Filesystem (VFS)

GalleonFS now includes a revolutionary cross-platform VFS that presents volumes as actual filesystems instead of block devices. This enables users to work with files and directories naturally while GalleonFS handles distributed storage, replication, and synchronization automatically.

### Key VFS Features

- **🖥️ Cross-Platform**: FUSE on Unix/Linux/macOS, WinFsp on Windows
- **⚡ Real-Time Sync**: Event-driven file monitoring with immediate block synchronization
- **🔄 Auto-Replication**: File changes automatically replicated across cluster nodes
- **📁 Natural Access**: Work with files/directories as regular filesystem
- **🚀 Auto-Mount**: Volumes automatically mounted when created
- **🛠️ CLI Management**: Full VFS control through CLI commands

### VFS Architecture

```
Application Layer   │ File Operations (read, write, mkdir, etc.)
                    │
VFS Interface       │ Cross-Platform VFS (FUSE/WinFsp/Fallback)
                    │
Event System        │ Real-Time File Change Detection
                    │
GalleonFS Core      │ Block Storage + Replication
                    │
Storage Layer       │ Distributed Storage Across Nodes
```

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
```
### VFS Operations

**Working with VFS-Mounted Volumes**:
```bash
# Start daemon with VFS auto-mounting
galleonfs -d --storage-path ./storage --mount-point ./mounts --bind-address 127.0.0.1:8081

# Create a volume (automatically mounted as VFS)
galleonfs volume create --name my-project --size 1G --storage-class fast-local-ssd

# The volume is now accessible as a regular directory
ls ./mounts/my-project/                    # List files in the volume
echo "Hello VFS!" > ./mounts/my-project/readme.txt    # Write to the volume
mkdir ./mounts/my-project/src              # Create directories
cp -r /local/files/* ./mounts/my-project/  # Copy files to distributed storage

# All file operations are immediately synced to GalleonFS and replicated across nodes
# Other cluster nodes will see the same files in their mount points
```

**VFS Management Commands**:
```bash
# List all VFS mounts
galleonfs vfs list

# Get VFS status for a volume
galleonfs vfs status my-project

# Force sync all pending writes (usually automatic)
galleonfs vfs sync

# Sync specific volume
galleonfs vfs sync my-project
```

**Cross-Node VFS Access**:
```bash
# On Node 1: Create file in VFS
echo "Created on Node 1" > /mnt/galleonfs/shared-vol/node1-file.txt

# On Node 2: File is automatically available
cat /mnt/galleonfs/shared-vol/node1-file.txt
# Output: Created on Node 1

# On Node 2: Create file in VFS  
echo "Created on Node 2" > /mnt/galleonfs/shared-vol/node2-file.txt

# On Node 1: File is automatically replicated
ls /mnt/galleonfs/shared-vol/
# Output: node1-file.txt node2-file.txt
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

### VFS Commands

```bash
galleonfs vfs <COMMAND>

Commands:
  mount      Mount a volume as a virtual filesystem
  unmount    Unmount a virtual filesystem  
  list       List all mounted virtual filesystems
  status     Get status of a mounted VFS
  sync       Force sync all pending writes to storage
```

**VFS Mount Options**:
```bash
galleonfs vfs mount <VOLUME> --mount-point <PATH> [OPTIONS]

Options:
  -m, --mount-point <PATH>       Mount point directory
  -r, --readonly                 Mount as read-only
```

**VFS Status Output**:
```bash
galleonfs vfs status my-volume

# Example output:
Volume: my-volume (a1b2c3d4-e5f6-7890-abcd-ef1234567890)
Mount Point: /mnt/galleonfs/my-volume
Status: Active
Files: 1,247
Directories: 42
Total Size: 2.3 GB
Last Sync: 2024-01-15 10:30:15 UTC
```

## Testing Scripts

### 🔬 VFS and File Replication Testing

GalleonFS includes comprehensive VFS testing scripts that validate the entire virtual filesystem functionality including real-time synchronization and cross-node file replication:

**Unix/Linux/macOS Testing**:
```bash
# Run the complete VFS test suite
./test-vfs.sh

# Optional: Use debug build for more verbose output
./test-vfs.sh --build debug

# View test progress in real-time
tail -f node1_daemon.log node2_daemon.log
```

**Windows Testing**:
```powershell
# Run the complete VFS test suite
.\test-vfs.ps1

# Optional: Use debug build and custom volume name
.\test-vfs.ps1 -BuildConfig debug -TestVolumeName "my-test-volume"

# View test progress in real-time
Get-Content Node1_daemon.log -Wait
```

**Comprehensive Test Coverage**:
- 🏗️ **Cluster Setup**: Automated two-node cluster initialization with Docker Swarm-style commands
- 📁 **VFS Mounting**: Cross-platform filesystem mounting (FUSE/WinFsp/Fallback modes)
- ✍️ **File Operations**: Create, read, write, delete, rename operations with immediate sync
- 📂 **Directory Operations**: Recursive directory creation, nested file structures
- ⚡ **Real-Time Sync**: Event-driven file change detection and cross-node replication
- 🔄 **Modification Testing**: Multiple file edits with line-by-line sync verification
- 🚀 **Performance Testing**: Batch file operations and replication rate measurement
- 🎛️ **VFS Commands**: Mount, unmount, status, sync command validation
- 🧹 **Cleanup Testing**: Graceful VFS unmounting and resource cleanup

**Test Validation**:
- ✅ **File Replication**: Verifies files written on Node 1 appear on Node 2
- ✅ **Event Processing**: Confirms real-time sync without timer polling
- ✅ **Cross-Node Consistency**: Validates identical file content across nodes
- ✅ **VFS Integration**: Tests filesystem-level operations through mounted volumes
- ✅ **Error Handling**: Graceful handling of FUSE/WinFsp availability
- ✅ **Resource Management**: Proper cleanup of processes and mount points

**Test Output Example**:
```
[INFO] Starting GalleonFS VFS and Replication Test
[SUCCESS] FUSE detected - full VFS functionality available  
[INFO] Building GalleonFS...
[SUCCESS] GalleonFS build completed
[INFO] Starting Node1 daemon...
[SUCCESS] Node1 daemon started (PID: 12345)
[INFO] Initializing cluster on Node 1...
[SUCCESS] Cluster initialized on Node 1
[INFO] Creating volume 'test-vfs-volume'...
[SUCCESS] Test volume created
[INFO] Test 1: Creating file on Node 1...
[SUCCESS] File created and readable on Node 1
[SUCCESS] File replicated successfully to Node 2
[INFO] Performance test: 9/10 files replicated to Node 2
[SUCCESS] Performance test shows good replication rate
[SUCCESS] VFS and replication testing completed!
```

The test scripts automatically handle:
- Building GalleonFS binaries
- Starting daemon processes with proper IPC configuration
- Cluster initialization and peer joining
- Volume creation with replication settings
- VFS mounting on both nodes
- File operation testing and verification
- Performance benchmarking
- Graceful cleanup and process termination

### Basic Cluster Testing

GalleonFS includes comprehensive testing scripts for automated cluster setup and testing:

### 🧪 **Automated Two-Node Cluster Testing**

Two testing scripts are provided for setting up and testing a complete two-node cluster:

- **`test-cluster.sh`**: Bash script for Linux/macOS
- **`test-cluster.ps1`**: PowerShell script for Windows

**Features:**
- ✅ Automated two-node cluster setup with different bind addresses
- ✅ Network replicated volume creation and verification  
- ✅ Comprehensive cluster status and peer verification
- ✅ Graceful cleanup and error handling
- ✅ Detailed logging and progress reporting
- ✅ Service readiness checking with timeout handling

### Running the Test Scripts

**Linux/macOS:**
```bash
# Make executable and run
chmod +x test-cluster.sh
./test-cluster.sh
```

**Windows PowerShell:**
```powershell
# Run with default settings
.\test-cluster.ps1

# Skip automatic building
.\test-cluster.ps1 -SkipBuild

# Keep storage files after completion
.\test-cluster.ps1 -KeepFiles
```

### What the Scripts Test

1. **Daemon Startup**: Starts two GalleonFS nodes in daemon mode
2. **Cluster Formation**: Verifies nodes join into a unified cluster
3. **IPC Communication**: Tests CLI communication with daemon services
4. **Volume Management**: Creates, lists, and queries replicated volumes
5. **Cross-Node Operations**: Verifies cluster operations from both nodes
6. **Cleanup**: Gracefully stops services and removes test data

### Script Output Example

```
[INFO] Starting GalleonFS Two-Node Cluster Test
[INFO] ========================================
[INFO] Configuration:
[INFO]   Node 1: Storage=./test_node1_storage, Bind=127.0.0.1:8081, IPC=127.0.0.1:8091
[INFO]   Node 2: Storage=./test_node2_storage, Bind=127.0.0.1:8082, IPC=127.0.0.1:8092
[INFO]   Volume: Name=test-replicated-volume, Size=1G, Replicas=2

[INFO] Step 1: Starting Node 1 (creates cluster)
[INFO] Node 1 started with PID: 1234
[SUCCESS] Node 1 IPC service is ready!

[INFO] Step 2: Starting Node 2 (joins cluster)  
[INFO] Node 2 started with PID: 1235
[SUCCESS] Node 2 IPC service is ready!

[INFO] Step 3: Checking cluster status
==========================
Cluster Status:
  Node ID: 13238112-0ac7-4042-9f96-97f7dba8b801
  Cluster Size: 2 nodes
  Peers:
    Node 2: 127.0.0.1:8082 (Connected)

[INFO] Step 4: Creating replicated volume
=================================
Volume created successfully:
  ID: f6bddca9-fb8b-4470-860b-dfde7b708e58
  Name: test-replicated-volume
  Type: Persistent
  Size: 1073741824 bytes (1.00 GB)
  Storage Class: encrypted-storage
  State: Available

[SUCCESS] Two-node cluster test completed successfully!
```

### Development & CI Integration

The testing scripts are designed for:

- **Local Development**: Quick verification of changes
- **CI/CD Pipelines**: Automated testing in build workflows  
- **Regression Testing**: Ensuring core functionality remains stable
- **Documentation Examples**: Demonstrating real-world usage patterns

### Customization

Both scripts can be easily customized by modifying the configuration variables at the top:

```bash
# Bash script configuration
NODE1_STORAGE="./test_node1_storage"
NODE2_STORAGE="./test_node2_storage"  
NODE1_BIND="127.0.0.1:8081"
NODE2_BIND="127.0.0.1:8082"
VOLUME_NAME="test-replicated-volume"
VOLUME_SIZE="1G"
REPLICAS=2
```

```powershell
# PowerShell script configuration
$NODE1_STORAGE = ".\test_node1_storage"
$NODE2_STORAGE = ".\test_node2_storage"
$NODE1_BIND = "127.0.0.1:8081"
$NODE2_BIND = "127.0.0.1:8082"
$VOLUME_NAME = "test-replicated-volume"
$VOLUME_SIZE = "1G"
$REPLICAS = 2
```

## VFS Technical Implementation

### Architecture Overview

The GalleonFS VFS system provides a unified interface for presenting distributed storage as regular filesystems across different operating systems:

```rust
// Core VFS trait for cross-platform compatibility
pub trait CrossPlatformVFS {
    async fn mount(&self, mount_point: &Path) -> Result<()>;
    async fn unmount(&self) -> Result<()>;
    async fn read(&self, ino: u64, offset: u64, size: u32) -> Result<Vec<u8>>;
    async fn write(&self, ino: u64, offset: u64, data: &[u8]) -> Result<u32>;
    // ... other filesystem operations
}
```

### Platform-Specific Implementations

**Unix/Linux/macOS (FUSE)**:
```rust
#[cfg(all(unix, feature = "fuse"))]
pub struct FuseVFS {
    vfs: GalleonVFS,
}

impl Filesystem for FuseVFS {
    fn write(&mut self, ino: u64, offset: i64, data: &[u8], reply: ReplyWrite) {
        // Real-time event generation for immediate sync
        let event = VFSEvent::FileModified { ino, offset, data: data.to_vec() };
        self.vfs.send_event(event).await;
        reply.written(data.len() as u32);
    }
}
```

**Windows (WinFsp - Planned)**:
```rust
#[cfg(windows)]
pub struct WinFspVFS {
    vfs: GalleonVFS,
}
// Full WinFsp integration planned for future releases
```

### Event-Driven Synchronization

The VFS uses a completely event-driven architecture with no timers:

```rust
pub enum VFSEvent {
    FileModified { path: PathBuf, ino: u64, offset: u64, data: Vec<u8> },
    FileCreated { path: PathBuf, ino: u64 },
    FileDeleted { path: PathBuf, ino: u64 },
    DirectoryCreated { path: PathBuf, ino: u64 },
    // ... other events
}

// Event processing pipeline
async fn process_vfs_event(galleonfs: &GalleonFS, volume_id: Uuid, event: VFSEvent) {
    match event {
        VFSEvent::FileModified { ino, offset, data, .. } => {
            // Immediate block-level write to GalleonFS
            let block_id = offset / BLOCK_SIZE;
            galleonfs.write_block(volume_id, block_id, &data, WriteConcern::WriteDurable).await?;
            
            // Trigger replication if needed
            if is_replicated_volume(volume_id) {
                trigger_replication(volume_id, block_id, &data).await?;
            }
        }
        // ... handle other events
    }
}
```

### Real-Time File Monitoring

File system events are captured immediately and processed without delays:

1. **File Write Detection**: FUSE/WinFsp captures write operations instantly
2. **Event Generation**: VFSEvent created with write details
3. **Block Translation**: File offset mapped to GalleonFS block ID
4. **Immediate Sync**: Data written to distributed storage
5. **Replication Trigger**: Cross-node replication initiated if configured

### Integration with GalleonFS Core

The VFS seamlessly integrates with existing GalleonFS functionality:

```rust
// Auto-mounting new volumes
async fn start_vfs_auto_mount(&self, base_mount_point: PathBuf) -> Result<()> {
    tokio::spawn(async move {
        loop {
            for volume in galleonfs.list_volumes().await? {
                if !vfs_map.contains_key(&volume.id) {
                    let mut vfs = create_cross_platform_vfs(volume.id, galleonfs.clone()).await?;
                    let mount_point = base_mount_point.join(&volume.name);
                    vfs.mount(&mount_point).await?;
                    vfs_map.insert(volume.id, vfs);
                }
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}
```

### Benefits Over Traditional Block Storage

1. **Natural File Operations**: Work with files/directories instead of raw blocks
2. **Cross-Platform Compatibility**: Same interface on Windows, Linux, macOS
3. **Immediate Consistency**: Real-time sync without polling or timers
4. **Transparent Replication**: File changes automatically replicated
5. **Developer Friendly**: Standard filesystem APIs for application integration

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

### VFS Testing Troubleshooting

**VFS Mount Failures**:
```bash
# Check FUSE availability (Linux/macOS)
which fusermount
ls -la /dev/fuse

# Install FUSE if missing (Ubuntu/Debian)
sudo apt-get install fuse3 libfuse3-dev

# Install FUSE if missing (macOS)
brew install --cask osxfuse

# Check WinFsp availability (Windows)
Get-Service -Name "WinFsp.Launcher"
```

**Test Script Issues**:
```bash
# Run with debug output for more detail
RUST_LOG=debug ./test-vfs.sh

# Check for port conflicts
netstat -tulnp | grep -E "(8081|8082|8091|8092)"

# Force cleanup if test fails
pkill -f galleonfs
rm -rf test_vfs_node*_storage test_vfs_node*_mount

# Windows cleanup
Get-Process -Name "*galleonfs*" | Stop-Process -Force
Remove-Item -Recurse -Force test_vfs_node*
```

**File Replication Issues**:
```bash
# Verify cluster connectivity
galleonfs cluster status

# Check VFS mount status
galleonfs vfs list

# Force synchronization
galleonfs vfs sync

# Check daemon logs for errors
tail -f node1_daemon.log node2_daemon.log
```

**Performance Issues**:
```bash
# Check available memory
free -h

# Monitor file system usage
watch -n 1 'galleonfs volume list; galleonfs vfs list'

# Check for storage conflicts
lsof | grep galleonfs
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
