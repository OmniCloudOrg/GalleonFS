# GalleonFS Architecture & Code Organization

This document describes the modular architecture and organization of GalleonFS, a high-performance distributed storage system.

## 🏗️ Core Architecture

GalleonFS follows a layered, modular architecture designed for enterprise-grade distributed storage:

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                        │
│  CLI, Tests, Demos, External Integrations                   │
├─────────────────────────────────────────────────────────────┤
│                  Storage Management Layer                   │
│  Volume Mounting, Migration, QoS Policies                   │
├─────────────────────────────────────────────────────────────┤
│                     Core Services Layer                     │
│  Volume Management, Snapshots, Backup/Recovery              │
├─────────────────────────────────────────────────────────────┤
│                   Replication & Security                    │
│  Data Replication, Encryption, Access Controls              │
├─────────────────────────────────────────────────────────────┤
│                    Storage Engine Layer                     │
│  Block Storage, Journaling, Consistency                     │
├─────────────────────────────────────────────────────────────┤
│                   Platform Integration                      │
│  FUSE (Linux/macOS), Virtual FS (Windows)                   │
└─────────────────────────────────────────────────────────────┘
```

## 📁 Module Organization

### Core Foundation (`src/lib.rs`)
**Purpose**: Central types, enums, and the main GalleonFS struct  
**Responsibilities**:
- Core data structures (Volume, StorageClass, VolumeState, etc.)
- Access modes and volume types
- Main GalleonFS orchestrator class
- Module integration and exports

**Key Types**:
- `GalleonFS` - Main storage system coordinator
- `Volume` - Volume metadata and lifecycle management
- `VolumeType` - Ephemeral, Persistent, Shared volumes
- `AccessMode` - ReadWriteOnce, ReadOnlyMany, ReadWriteMany

### Storage Engine (`src/storage.rs`)
**Purpose**: Low-level block storage, journaling, and data persistence  
**Responsibilities**:
- Block-level read/write operations
- Write-ahead journaling for crash consistency  
- Data integrity with checksums
- Storage abstraction layer

**Key Components**:
- `StorageEngine` trait - Pluggable storage backend interface
- `FileStorageEngine` - Local filesystem implementation
- `JournalEntry` - Transaction logging for consistency
- Block-level operations with configurable block sizes

### Volume Management (`src/volume.rs`)  
**Purpose**: Volume lifecycle, metadata, and high-level operations
**Responsibilities**:
- Volume creation, deletion, expansion
- Volume cloning and transformation
- Metadata management
- Volume state transitions

**Key Features**:
- Dynamic volume provisioning
- Storage class-based volume creation
- Volume expansion (online and offline)
- Volume cloning with copy-on-write

### Volume Mounting (`src/volume_mount.rs`)
**Purpose**: Individual volume mounting for filesystem access
**Responsibilities**:
- Per-volume mount point management
- Multiple concurrent mount support
- Mount state tracking and lifecycle
- Application filesystem interface

**Key Components**:
- `VolumeMountManager` - Mount orchestration
- `VolumeMount` - Mount metadata and state
- `GalleonVolumeFile` - File handle for volume operations
- Smart multiple mount policies based on volume characteristics

### Snapshot Management (`src/snapshot.rs`)
**Purpose**: Point-in-time volume snapshots and recovery
**Responsibilities**:
- Snapshot creation and deletion
- Copy-on-write implementation
- Snapshot metadata management
- Point-in-time recovery operations

**Key Features**:
- Incremental snapshot support
- Snapshot chains and dependencies
- Fast snapshot creation with metadata-only operations
- Snapshot-based volume cloning

### Backup & Recovery (`src/backup.rs`)
**Purpose**: Data protection, backup strategies, and disaster recovery
**Responsibilities**:
- Multiple backup strategies (Full, Incremental, Differential)
- Backup scheduling and retention policies
- Cross-region backup support
- Application-consistent backups

**Key Components**:
- `BackupController` - Backup orchestration
- `BackupStrategy` - Different backup approaches
- `BackupPolicy` - Automated backup rules
- `RecoveryManager` - Point-in-time recovery

### Replication (`src/replication.rs`)  
**Purpose**: Data redundancy and distributed consistency
**Responsibilities**:
- Synchronous and asynchronous replication
- Cross-node data synchronization
- Replication topology management
- Consistency guarantees

**Key Features**:
- Configurable replication strategies
- Multi-node cluster support
- Conflict resolution for concurrent writes
- Network partition tolerance

### Migration (`src/migration.rs`)
**Purpose**: Live volume migration between storage classes and nodes
**Responsibilities**:
- Online and offline migration
- Cross-storage-class migrations
- Node-to-node volume movement
- Migration progress tracking

**Key Components**:
- `MigrationManager` - Migration orchestration
- `MigrationStrategy` - Different migration approaches
- Progress tracking and rollback capabilities
- Minimal downtime migrations

### Quality of Service (`src/qos.rs`)
**Purpose**: Performance guarantees and resource management
**Responsibilities**:
- IOPS and throughput limits/guarantees
- Latency SLA enforcement
- Burstable performance policies
- Resource scheduling and prioritization

**Key Features**:
- Per-volume QoS policies
- Dynamic QoS adjustment
- Performance monitoring integration
- Fair queuing and prioritization

### Security (`src/security.rs`)
**Purpose**: Encryption, access control, and audit logging
**Responsibilities**:
- Encryption at rest (AES-256-GCM)
- Key management (Internal, External, HSM)
- Access control policies
- Audit trail and compliance

**Key Components**:
- `EncryptionManager` - Crypto operations
- `VolumeAccessPolicy` - Fine-grained access control
- `AuditLogger` - Security event logging
- Multi-tenant isolation

### Monitoring (`src/monitoring.rs`)
**Purpose**: Performance metrics, telemetry, and observability
**Responsibilities**:
- Real-time performance metrics (IOPS, throughput, latency)
- Resource utilization tracking
- Health monitoring and alerting
- Prometheus metrics integration

**Key Metrics**:
- Volume-level performance indicators
- Storage engine health metrics
- Replication lag and status
- System resource utilization

## 🖥️ Platform Integration

### FUSE Filesystem (`src/fuse_fs.rs`) - Linux/macOS
**Purpose**: POSIX-compliant filesystem interface
**Responsibilities**:
- Standard filesystem operations (read, write, seek)
- Directory structure and metadata
- File handle management
- POSIX permission integration

### Virtual Filesystem (`src/virtual_fs.rs`) - Windows  
**Purpose**: Windows-compatible volume access
**Responsibilities**:
- Volume-as-file interface
- Windows path integration
- Application compatibility layer
- Directory-based volume organization

## 🧪 Testing & Validation

### Integration Tests (`tests/integration_tests.rs`)
**Purpose**: End-to-end system validation
**Coverage**:
- Volume lifecycle operations
- Replication and consistency
- Performance characteristics
- Error handling and recovery

### Replication Tests (`src/replication_mount_test.rs`)
**Purpose**: Multi-node replication and mounting validation
**Scenarios**:
- Cross-node data replication
- Multiple concurrent mounts
- Consistency verification
- Performance under load

## 🎯 Design Principles

### 1. **Modularity**
Each module has a single, well-defined responsibility with minimal cross-dependencies. This enables:
- Independent testing and validation
- Easy feature addition and modification
- Clear separation of concerns
- Maintainable codebase

### 2. **Async/Await Throughout**
All I/O operations use Rust's async/await for maximum performance:
- Non-blocking storage operations
- Concurrent request handling
- Efficient resource utilization
- Scalable multi-client support

### 3. **Strong Type Safety**
Leverages Rust's type system for correctness:
- Compile-time error prevention
- Clear API contracts
- Memory safety guarantees
- Zero-cost abstractions

### 4. **Pluggable Architecture**
Key interfaces are trait-based for extensibility:
- `StorageEngine` trait for different backends
- `ReplicationStrategy` for various replication modes
- `BackupStrategy` for different backup approaches
- Platform-specific filesystem integrations

### 5. **Enterprise-Grade Features**
Production-ready capabilities from day one:
- Comprehensive error handling with `anyhow::Result`
- Structured logging with `tracing`
- Performance monitoring and metrics
- Security and compliance features

## 🔄 Data Flow

### Write Path
```
Application → Volume Mount → Volume Manager → Storage Engine → Journal → Disk
                                    ↓
                        Replication Service → Peer Nodes
```

### Read Path  
```
Application → Volume Mount → Volume Manager → Storage Engine → Disk
                                    ↓
                        Cache/Performance Optimization
```

### Backup Path
```
Backup Controller → Snapshot Manager → Storage Engine → Backup Target
                                    ↓
                        Retention Policy → Cleanup
```

This modular architecture ensures GalleonFS can scale from single-node deployments to large distributed clusters while maintaining code clarity, performance, and reliability.