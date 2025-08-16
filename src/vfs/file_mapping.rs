use anyhow::Result;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, BTreeMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::{VFS_BLOCK_SIZE};

/// File-to-block mapping system for efficient VFS operations
/// Maps file system operations to underlying VFS block storage
pub struct FileMapper {
    /// File inode to VFS block mappings
    inode_mappings: Arc<RwLock<HashMap<u64, InodeMapping>>>,
    /// Path to inode mappings for quick lookups
    path_mappings: Arc<RwLock<HashMap<PathBuf, u64>>>,
    /// Block allocation tracking per volume
    block_allocations: Arc<RwLock<HashMap<Uuid, VolumeBlockAllocation>>>,
    /// File system metadata
    fs_metadata: Arc<RwLock<VfsMetadata>>,
    /// Next available inode number
    next_inode: Arc<RwLock<u64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InodeMapping {
    /// File system inode number
    pub inode: u64,
    /// Volume this file belongs to
    pub volume_id: Uuid,
    /// File path within volume
    pub path: PathBuf,
    /// File type (file, directory, symlink)
    pub file_type: VfsFileType,
    /// File size in bytes
    pub size: u64,
    /// Block mappings for this file
    pub block_mappings: BTreeMap<u64, BlockMapping>, // file_offset -> block_mapping
    /// File creation time
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// File modification time
    pub modified_at: chrono::DateTime<chrono::Utc>,
    /// File access time
    pub accessed_at: chrono::DateTime<chrono::Utc>,
    /// File permissions
    pub permissions: FilePermissions,
    /// Extended attributes
    pub extended_attributes: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VfsFileType {
    RegularFile,
    Directory,
    SymbolicLink,
    BlockDevice,
    CharDevice,
    Fifo,
    Socket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePermissions {
    /// Unix-style permissions (rwxrwxrwx)
    pub mode: u32,
    /// Owner user ID
    pub uid: u32,
    /// Owner group ID
    pub gid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockMapping {
    /// VFS shard ID
    pub shard_id: u32,
    /// Block ID within shard
    pub block_id: u64,
    /// Offset within the block where file data starts
    pub block_offset: u64,
    /// Length of data in this block for this file
    pub length: u64,
    /// Whether this block is fully allocated to this file
    pub is_full_block: bool,
    /// Compression status
    pub compression_info: Option<CompressionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionInfo {
    /// Compression algorithm used
    pub algorithm: String,
    /// Original size before compression
    pub original_size: u64,
    /// Compressed size
    pub compressed_size: u64,
    /// Compression ratio
    pub compression_ratio: f64,
}

#[derive(Debug, Clone)]
pub struct VolumeBlockAllocation {
    /// Volume ID
    pub volume_id: Uuid,
    /// Free blocks per shard
    pub free_blocks_per_shard: HashMap<u32, u64>,
    /// Allocation bitmap per shard (for fast free block lookup)
    pub allocation_bitmaps: HashMap<u32, Vec<u64>>, // shard_id -> bitmap
    /// Block allocation strategy
    pub allocation_strategy: AllocationStrategy,
    /// Defragmentation score (0.0 = perfectly defragmented, 1.0 = highly fragmented)
    pub fragmentation_score: f64,
}

#[derive(Debug, Clone)]
pub enum AllocationStrategy {
    /// First-fit allocation
    FirstFit,
    /// Best-fit allocation (minimizes fragmentation)
    BestFit,
    /// Worst-fit allocation (spreads data)
    WorstFit,
    /// Sequential allocation (for streaming workloads)
    Sequential,
    /// Random allocation (for parallel workloads)
    Random,
}

#[derive(Debug, Clone)]
pub struct VfsMetadata {
    /// Volume ID this metadata belongs to
    pub volume_id: Uuid,
    /// Root directory inode
    pub root_inode: u64,
    /// Total files in this volume
    pub total_files: u64,
    /// Total directories in this volume
    pub total_directories: u64,
    /// Total size of all files
    pub total_file_size: u64,
    /// Volume mount options
    pub mount_options: MountOptions,
}

#[derive(Debug, Clone)]
pub struct MountOptions {
    /// Read-only mount
    pub readonly: bool,
    /// Enable compression
    pub enable_compression: bool,
    /// Enable access time updates
    pub enable_atime: bool,
    /// Enable directory access time updates
    pub enable_diratime: bool,
    /// Synchronous writes
    pub sync_writes: bool,
}

/// Result of mapping file operations to blocks
#[derive(Debug, Clone)]
pub struct FileMappingResult {
    /// Affected block mappings
    pub block_mappings: Vec<BlockMapping>,
    /// Whether new blocks were allocated
    pub new_allocations: bool,
    /// Whether existing blocks were modified
    pub modified_blocks: bool,
    /// Total bytes affected
    pub bytes_affected: u64,
}

impl FileMapper {
    pub async fn new() -> Result<Self> {
        info!("🗺️ Initializing file-to-block mapping system");

        Ok(Self {
            inode_mappings: Arc::new(RwLock::new(HashMap::new())),
            path_mappings: Arc::new(RwLock::new(HashMap::new())),
            block_allocations: Arc::new(RwLock::new(HashMap::new())),
            fs_metadata: Arc::new(RwLock::new(VfsMetadata {
                volume_id: Uuid::new_v4(), // Will be set per volume
                root_inode: 1,
                total_files: 0,
                total_directories: 0,
                total_file_size: 0,
                mount_options: MountOptions {
                    readonly: false,
                    enable_compression: true,
                    enable_atime: true,
                    enable_diratime: false,
                    sync_writes: false,
                },
            })),
            next_inode: Arc::new(RwLock::new(2)), // Start from 2 (1 is reserved for root)
        })
    }

    /// Initialize mapping for a new volume
    pub async fn initialize_volume(&self, volume_id: Uuid, shard_count: u32) -> Result<()> {
        info!("📋 Initializing file mappings for volume: {}", volume_id);

        // Create volume block allocation tracking
        let mut free_blocks_per_shard = HashMap::new();
        let mut allocation_bitmaps = HashMap::new();

        for shard_id in 0..shard_count {
            // Initialize each shard as fully free
            free_blocks_per_shard.insert(shard_id, super::MAX_BLOCKS_PER_SHARD);
            allocation_bitmaps.insert(shard_id, Vec::new());
        }

        let volume_allocation = VolumeBlockAllocation {
            volume_id,
            free_blocks_per_shard,
            allocation_bitmaps,
            allocation_strategy: AllocationStrategy::BestFit,
            fragmentation_score: 0.0,
        };

        {
            let mut allocations = self.block_allocations.write().await;
            allocations.insert(volume_id, volume_allocation);
        }

        // Create root directory mapping
        self.create_root_directory(volume_id).await?;

        info!("✅ Volume file mappings initialized");
        Ok(())
    }

    /// Map file operations to VFS blocks
    pub async fn map_file_to_blocks(
        &self,
        volume_id: Uuid,
        file_path: &Path,
        offset: u64,
        length: u64,
    ) -> Result<Vec<BlockMapping>> {
        debug!("🗺️ Mapping file operation: path={:?}, offset={}, length={}", 
               file_path, offset, length);

        // Get or create inode for this file
        let inode = self.get_or_create_inode(volume_id, file_path).await?;

        // Calculate which blocks are affected
        let start_block = offset / VFS_BLOCK_SIZE;
        let end_block = (offset + length - 1) / VFS_BLOCK_SIZE;

        let mut block_mappings = Vec::new();

        for file_block_index in start_block..=end_block {
            let file_offset_in_block = if file_block_index == start_block {
                offset % VFS_BLOCK_SIZE
            } else {
                0
            };

            let remaining_length = if file_block_index == end_block {
                (offset + length) - (file_block_index * VFS_BLOCK_SIZE) - file_offset_in_block
            } else {
                VFS_BLOCK_SIZE - file_offset_in_block
            };

            // Get existing block mapping or allocate new one
            let block_mapping = self.get_or_allocate_block(
                volume_id,
                inode,
                file_block_index,
                file_offset_in_block,
                remaining_length,
            ).await?;

            block_mappings.push(block_mapping);
        }

        debug!("✅ Mapped to {} blocks", block_mappings.len());
        Ok(block_mappings)
    }

    /// Get or create an inode for a file path
    pub async fn get_or_create_inode(&self, volume_id: Uuid, file_path: &Path) -> Result<u64> {
        // Check if we already have a mapping for this path
        {
            let path_mappings = self.path_mappings.read().await;
            if let Some(inode) = path_mappings.get(file_path) {
                return Ok(*inode);
            }
        }

        // Create new inode
        let inode = {
            let mut next_inode = self.next_inode.write().await;
            let current = *next_inode;
            *next_inode += 1;
            current
        };

        // Create inode mapping
        let inode_mapping = InodeMapping {
            inode,
            volume_id,
            path: file_path.to_path_buf(),
            file_type: VfsFileType::RegularFile, // TODO: Detect actual file type
            size: 0,
            block_mappings: BTreeMap::new(),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            accessed_at: chrono::Utc::now(),
            permissions: FilePermissions {
                mode: 0o644, // Default file permissions
                uid: 1000,   // Default user
                gid: 1000,   // Default group
            },
            extended_attributes: HashMap::new(),
        };

        // Store mappings
        {
            let mut inode_mappings = self.inode_mappings.write().await;
            inode_mappings.insert(inode, inode_mapping);
        }

        {
            let mut path_mappings = self.path_mappings.write().await;
            path_mappings.insert(file_path.to_path_buf(), inode);
        }

        debug!("📁 Created new inode {} for path: {:?}", inode, file_path);
        Ok(inode)
    }

    /// Get existing block mapping or allocate a new block
    async fn get_or_allocate_block(
        &self,
        volume_id: Uuid,
        inode: u64,
        file_block_index: u64,
        block_offset: u64,
        length: u64,
    ) -> Result<BlockMapping> {
        // Check if block already exists for this file
        {
            let inode_mappings = self.inode_mappings.read().await;
            if let Some(inode_mapping) = inode_mappings.get(&inode) {
                let file_offset = file_block_index * VFS_BLOCK_SIZE;
                if let Some(existing_mapping) = inode_mapping.block_mappings.get(&file_offset) {
                    return Ok(existing_mapping.clone());
                }
            }
        }

        // Allocate new block
        let (shard_id, block_id) = self.allocate_new_block(volume_id).await?;

        let block_mapping = BlockMapping {
            shard_id,
            block_id,
            block_offset,
            length,
            is_full_block: block_offset == 0 && length == VFS_BLOCK_SIZE,
            compression_info: None,
        };

        // Update inode mapping
        {
            let mut inode_mappings = self.inode_mappings.write().await;
            if let Some(inode_mapping) = inode_mappings.get_mut(&inode) {
                let file_offset = file_block_index * VFS_BLOCK_SIZE;
                inode_mapping.block_mappings.insert(file_offset, block_mapping.clone());
                inode_mapping.modified_at = chrono::Utc::now();
                
                // Update file size if this extends the file
                let new_end = file_offset + length;
                if new_end > inode_mapping.size {
                    inode_mapping.size = new_end;
                }
            }
        }

        debug!("🆕 Allocated new block: shard={}, block={}, offset={}, length={}", 
               shard_id, block_id, block_offset, length);

        Ok(block_mapping)
    }

    /// Allocate a new block using the configured allocation strategy
    async fn allocate_new_block(&self, volume_id: Uuid) -> Result<(u32, u64)> {
        let mut allocations = self.block_allocations.write().await;
        
        let volume_allocation = allocations.get_mut(&volume_id)
            .ok_or_else(|| anyhow::anyhow!("Volume not found: {}", volume_id))?;

        // Find shard with available blocks
        let shard_candidates: Vec<_> = volume_allocation.free_blocks_per_shard.iter()
            .filter(|(_, free_blocks)| **free_blocks > 0)
            .map(|(shard_id, _)| *shard_id)
            .collect();

        for shard_id in shard_candidates {
            // Allocate block using strategy
            let block_id = match volume_allocation.allocation_strategy {
                AllocationStrategy::FirstFit => self.allocate_first_fit(shard_id).await?,
                AllocationStrategy::BestFit => self.allocate_best_fit(shard_id).await?,
                AllocationStrategy::Sequential => self.allocate_sequential(shard_id).await?,
                _ => self.allocate_first_fit(shard_id).await?, // Default to first-fit
            };

            // Update free block count
            if let Some(free_count) = volume_allocation.free_blocks_per_shard.get_mut(&shard_id) {
                *free_count = free_count.saturating_sub(1);
            }

            return Ok((shard_id, block_id));
        }

        Err(anyhow::anyhow!("No free blocks available in volume: {}", volume_id))
    }

    async fn allocate_first_fit(&self, _shard_id: u32) -> Result<u64> {
        // Simple first-fit: return next available block
        // TODO: Implement proper bitmap-based allocation
        let mut rng = rand::thread_rng();
        Ok(rng.gen::<u64>() % super::MAX_BLOCKS_PER_SHARD)
    }

    async fn allocate_best_fit(&self, shard_id: u32) -> Result<u64> {
        // TODO: Implement best-fit allocation to minimize fragmentation
        self.allocate_first_fit(shard_id).await
    }

    async fn allocate_sequential(&self, shard_id: u32) -> Result<u64> {
        // TODO: Implement sequential allocation for streaming workloads
        self.allocate_first_fit(shard_id).await
    }

    /// Create root directory for a volume
    async fn create_root_directory(&self, volume_id: Uuid) -> Result<()> {
        let root_inode = 1;
        let root_path = PathBuf::from("/");

        let root_mapping = InodeMapping {
            inode: root_inode,
            volume_id,
            path: root_path.clone(),
            file_type: VfsFileType::Directory,
            size: VFS_BLOCK_SIZE, // Directories get one block by default
            block_mappings: BTreeMap::new(),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            accessed_at: chrono::Utc::now(),
            permissions: FilePermissions {
                mode: 0o755, // Directory permissions
                uid: 0,      // Root user
                gid: 0,      // Root group
            },
            extended_attributes: HashMap::new(),
        };

        // Allocate root directory block
        let (shard_id, block_id) = self.allocate_new_block(volume_id).await?;
        let root_block_mapping = BlockMapping {
            shard_id,
            block_id,
            block_offset: 0,
            length: VFS_BLOCK_SIZE,
            is_full_block: true,
            compression_info: None,
        };

        // Store mappings
        {
            let mut inode_mappings = self.inode_mappings.write().await;
            let mut root_with_block = root_mapping;
            root_with_block.block_mappings.insert(0, root_block_mapping);
            inode_mappings.insert(root_inode, root_with_block);
        }

        {
            let mut path_mappings = self.path_mappings.write().await;
            path_mappings.insert(root_path, root_inode);
        }

        info!("📁 Created root directory for volume: {}", volume_id);
        Ok(())
    }

    /// Get file information by path
    pub async fn get_file_info(&self, file_path: &Path) -> Result<Option<InodeMapping>> {
        let path_mappings = self.path_mappings.read().await;
        if let Some(inode) = path_mappings.get(file_path) {
            let inode_mappings = self.inode_mappings.read().await;
            Ok(inode_mappings.get(inode).cloned())
        } else {
            Ok(None)
        }
    }

    /// Update file access time
    pub async fn update_access_time(&self, file_path: &Path) -> Result<()> {
        let inode = {
            let path_mappings = self.path_mappings.read().await;
            path_mappings.get(file_path).copied()
        };

        if let Some(inode) = inode {
            let mut inode_mappings = self.inode_mappings.write().await;
            if let Some(mapping) = inode_mappings.get_mut(&inode) {
                mapping.accessed_at = chrono::Utc::now();
            }
        }

        Ok(())
    }

    /// Remove file mapping
    pub async fn remove_file(&self, file_path: &Path) -> Result<()> {
        let inode = {
            let mut path_mappings = self.path_mappings.write().await;
            path_mappings.remove(file_path)
        };

        if let Some(inode) = inode {
            // Get block mappings to deallocate
            let block_mappings = {
                let mut inode_mappings = self.inode_mappings.write().await;
                if let Some(mapping) = inode_mappings.remove(&inode) {
                    mapping.block_mappings
                } else {
                    BTreeMap::new()
                }
            };

            // Deallocate blocks
            for (_, block_mapping) in block_mappings {
                self.deallocate_block(block_mapping.shard_id, block_mapping.block_id).await?;
            }

            info!("🗑️ Removed file mapping: {:?}", file_path);
        }

        Ok(())
    }

    async fn deallocate_block(&self, _shard_id: u32, _block_id: u64) -> Result<()> {
        // TODO: Implement block deallocation
        // This would mark blocks as free in the allocation bitmap
        Ok(())
    }

    /// Get volume statistics
    pub async fn get_volume_stats(&self, volume_id: Uuid) -> Result<VolumeStats> {
        let inode_mappings = self.inode_mappings.read().await;
        
        let mut total_files = 0;
        let mut total_directories = 0;
        let mut total_size = 0;
        let mut total_blocks = 0;

        for mapping in inode_mappings.values() {
            if mapping.volume_id == volume_id {
                match mapping.file_type {
                    VfsFileType::RegularFile => total_files += 1,
                    VfsFileType::Directory => total_directories += 1,
                    _ => {}
                }
                total_size += mapping.size;
                total_blocks += mapping.block_mappings.len() as u64;
            }
        }

        Ok(VolumeStats {
            volume_id,
            total_files,
            total_directories,
            total_size,
            total_blocks,
            fragmentation_score: 0.0, // TODO: Calculate actual fragmentation
        })
    }
}

#[derive(Debug, Clone)]
pub struct VolumeStats {
    pub volume_id: Uuid,
    pub total_files: u64,
    pub total_directories: u64,
    pub total_size: u64,
    pub total_blocks: u64,
    pub fragmentation_score: f64,
}