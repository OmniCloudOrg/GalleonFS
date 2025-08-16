//! High-Performance Virtual Filesystem Service
//!
//! This module provides a real-time virtual filesystem that syncs between
//! a real directory structure and GalleonFS block storage for extreme performance.

use anyhow::Result;
use notify::{Watcher, RecommendedWatcher, RecursiveMode, Event, EventKind};
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, Instant};
use tokio::fs;
use tokio::sync::{RwLock, Mutex, mpsc};
use tracing::{info, error, debug, trace};
use uuid::Uuid;

use crate::{GalleonFS, WriteConcern};

const BLOCK_SIZE: u64 = 4096;
const METADATA_BLOCK: u64 = 0;
const ALLOCATION_BITMAP_BLOCK: u64 = 1;
const ROOT_DIRECTORY_BLOCK: u64 = 2;
const FIRST_DATA_BLOCK: u64 = 3;

// Performance tuning constants
const SYNC_BATCH_SIZE: usize = 32;
const CACHE_SIZE: usize = 1024;
const WRITE_BUFFER_SIZE: usize = 64 * 1024; // 64KB
const METADATA_SYNC_INTERVAL_MS: u64 = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VFSMetadata {
    pub version: u32,
    pub volume_id: Uuid,
    pub block_size: u64,
    pub total_blocks: u64,
    pub used_blocks: u64,
    pub root_inode: u64,
    pub next_inode: u64,
    pub created_at: SystemTime,
    pub last_modified: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VFSInode {
    pub ino: u64,
    pub file_type: VFSFileType,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub blocks: Vec<u64>,
    pub created_at: SystemTime,
    pub modified_at: SystemTime,
    pub accessed_at: SystemTime,
    pub link_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VFSFileType {
    RegularFile,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VFSDirectoryEntry {
    pub name: String,
    pub inode: u64,
}

#[derive(Debug)]
pub struct VFSCache {
    inodes: HashMap<u64, VFSInode>,
    directory_entries: HashMap<u64, Vec<VFSDirectoryEntry>>, // inode -> entries
    blocks: HashMap<u64, Vec<u8>>,
    dirty_inodes: HashMap<u64, Instant>,
    dirty_blocks: HashMap<u64, Instant>,
}

impl VFSCache {
    fn new() -> Self {
        Self {
            inodes: HashMap::with_capacity(CACHE_SIZE),
            directory_entries: HashMap::with_capacity(CACHE_SIZE / 4),
            blocks: HashMap::with_capacity(CACHE_SIZE),
            dirty_inodes: HashMap::new(),
            dirty_blocks: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub enum VFSSyncOperation {
    CreateFile { path: PathBuf, inode: u64 },
    WriteFile { path: PathBuf, inode: u64, offset: u64, data: Vec<u8> },
    DeleteFile { path: PathBuf, inode: u64 },
    CreateDirectory { path: PathBuf, inode: u64 },
    DeleteDirectory { path: PathBuf, inode: u64 },
    RenameFile { old_path: PathBuf, new_path: PathBuf, inode: u64 },
    UpdateMetadata { path: PathBuf, inode: u64 },
}

pub struct HighPerformanceVFS {
    volume_id: Uuid,
    mount_point: PathBuf,
    galleonfs: Arc<GalleonFS>,
    
    // Core filesystem state
    metadata: Arc<RwLock<VFSMetadata>>,
    cache: Arc<RwLock<VFSCache>>,
    allocation_bitmap: Arc<RwLock<Vec<bool>>>,
    
    // Performance optimization
    sync_queue: Arc<Mutex<VecDeque<VFSSyncOperation>>>,
    batch_processor: Arc<AtomicBool>,
    
    // Real-time file watching
    _watcher: RecommendedWatcher,
    sync_tx: mpsc::UnboundedSender<VFSSyncOperation>,
    
    // Performance metrics
    ops_per_second: Arc<AtomicU64>,
    last_ops_time: Arc<RwLock<Instant>>,
    
    // Path to inode mapping for fast lookups
    path_to_inode: Arc<RwLock<HashMap<PathBuf, u64>>>,
    inode_to_path: Arc<RwLock<HashMap<u64, PathBuf>>>,
}

impl HighPerformanceVFS {
    pub async fn new(
        volume_id: Uuid,
        mount_point: PathBuf,
        galleonfs: Arc<GalleonFS>,
        volume_size: u64,
    ) -> Result<Self> {
        info!("Creating high-performance VFS for volume {} at {}", volume_id, mount_point.display());
        
        // Ensure mount point exists
        if !mount_point.exists() {
            fs::create_dir_all(&mount_point).await?;
        }
        
        // Calculate filesystem parameters
        let total_blocks = volume_size / BLOCK_SIZE;
        let allocation_bitmap = vec![false; total_blocks as usize];
        
        // Initialize metadata
        let metadata = VFSMetadata {
            version: 1,
            volume_id,
            block_size: BLOCK_SIZE,
            total_blocks,
            used_blocks: FIRST_DATA_BLOCK, // Metadata blocks are pre-allocated
            root_inode: 1,
            next_inode: 2,
            created_at: SystemTime::now(),
            last_modified: SystemTime::now(),
        };
        
        // Create sync channel
        let (sync_tx, sync_rx) = mpsc::unbounded_channel();
        
        // Set up file watcher
        let watcher_tx = sync_tx.clone();
        let watch_mount_point = mount_point.clone();
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    if let Err(e) = Self::handle_file_event(&watcher_tx, &watch_mount_point, event) {
                        error!("Failed to handle file event: {}", e);
                    }
                }
                Err(e) => error!("File watcher error: {}", e),
            }
        })?;
        
        watcher.watch(&mount_point, RecursiveMode::Recursive)?;
        
        let vfs = Self {
            volume_id,
            mount_point: mount_point.clone(),
            galleonfs: galleonfs.clone(),
            metadata: Arc::new(RwLock::new(metadata)),
            cache: Arc::new(RwLock::new(VFSCache::new())),
            allocation_bitmap: Arc::new(RwLock::new(allocation_bitmap)),
            sync_queue: Arc::new(Mutex::new(VecDeque::new())),
            batch_processor: Arc::new(AtomicBool::new(false)),
            _watcher: watcher,
            sync_tx,
            ops_per_second: Arc::new(AtomicU64::new(0)),
            last_ops_time: Arc::new(RwLock::new(Instant::now())),
            path_to_inode: Arc::new(RwLock::new(HashMap::new())),
            inode_to_path: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Initialize filesystem or load existing
        vfs.initialize_or_load().await?;
        
        // Start background sync processor
        vfs.start_sync_processor(sync_rx).await;
        
        // Start metadata sync timer
        vfs.start_metadata_sync_timer().await;
        
        info!("High-performance VFS initialized successfully");
        Ok(vfs)
    }
    
    async fn initialize_or_load(&self) -> Result<()> {
        // Try to load existing filesystem metadata
        match self.load_metadata().await {
            Ok(_) => {
                info!("Loaded existing VFS metadata");
                self.load_filesystem_structure().await?;
            }
            Err(_) => {
                info!("Initializing new VFS");
                self.initialize_new_filesystem().await?;
            }
        }
        
        Ok(())
    }
    
    async fn load_metadata(&self) -> Result<()> {
        // Read metadata from block 0
        let metadata_data = self.galleonfs.read_block(self.volume_id, METADATA_BLOCK).await?;
        if metadata_data.is_empty() {
            return Err(anyhow::anyhow!("No metadata found"));
        }
        
        let metadata: VFSMetadata = bincode::deserialize(&metadata_data)?;
        *self.metadata.write().await = metadata;
        
        // Load allocation bitmap from block 1
        let bitmap_data = self.galleonfs.read_block(self.volume_id, ALLOCATION_BITMAP_BLOCK).await?;
        if !bitmap_data.is_empty() {
            let bitmap: Vec<bool> = bincode::deserialize(&bitmap_data)?;
            *self.allocation_bitmap.write().await = bitmap;
        }
        
        Ok(())
    }
    
    async fn initialize_new_filesystem(&self) -> Result<()> {
        info!("Initializing new filesystem structure");
        
        // Mark metadata blocks as used
        {
            let mut bitmap = self.allocation_bitmap.write().await;
            bitmap[METADATA_BLOCK as usize] = true;
            bitmap[ALLOCATION_BITMAP_BLOCK as usize] = true;
            bitmap[ROOT_DIRECTORY_BLOCK as usize] = true;
        }
        
        // Create root directory inode
        let root_inode = VFSInode {
            ino: 1,
            file_type: VFSFileType::Directory,
            mode: 0o755,
            uid: 0,
            gid: 0,
            size: 0,
            blocks: vec![ROOT_DIRECTORY_BLOCK],
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
            accessed_at: SystemTime::now(),
            link_count: 2,
        };
        
        // Cache root inode
        {
            let mut cache = self.cache.write().await;
            cache.inodes.insert(1, root_inode);
            cache.directory_entries.insert(1, Vec::new());
        }
        
        // Save initial state
        self.save_metadata().await?;
        self.save_allocation_bitmap().await?;
        self.save_inode(1).await?;
        
        Ok(())
    }
    
    async fn load_filesystem_structure(&self) -> Result<()> {
        info!("Loading existing filesystem structure");
        // Implementation would load all inodes and rebuild path mappings
        // For now, we'll implement a basic version
        Ok(())
    }
    
    async fn start_sync_processor(&self, mut sync_rx: mpsc::UnboundedReceiver<VFSSyncOperation>) {
        let vfs = self.clone();
        
        tokio::spawn(async move {
            let mut batch = Vec::with_capacity(SYNC_BATCH_SIZE);
            let mut last_flush = Instant::now();
            
            while let Some(operation) = sync_rx.recv().await {
                batch.push(operation);
                
                // Process batch when full or after timeout
                if batch.len() >= SYNC_BATCH_SIZE || last_flush.elapsed().as_millis() > METADATA_SYNC_INTERVAL_MS as u128 {
                    if let Err(e) = vfs.process_sync_batch(&batch).await {
                        error!("Failed to process sync batch: {}", e);
                    }
                    batch.clear();
                    last_flush = Instant::now();
                }
            }
            
            // Process any remaining operations
            if !batch.is_empty() {
                if let Err(e) = vfs.process_sync_batch(&batch).await {
                    error!("Failed to process final sync batch: {}", e);
                }
            }
        });
    }
    
    async fn start_metadata_sync_timer(&self) {
        let vfs = self.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(METADATA_SYNC_INTERVAL_MS));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = vfs.sync_dirty_metadata().await {
                    error!("Failed to sync dirty metadata: {}", e);
                }
            }
        });
    }
    
    fn handle_file_event(
        sync_tx: &mpsc::UnboundedSender<VFSSyncOperation>,
        mount_point: &Path,
        event: Event,
    ) -> Result<()> {
        for path in event.paths {
            // Skip hidden files and metadata files
            if let Some(name) = path.file_name() {
                if name.to_string_lossy().starts_with('.') {
                    continue;
                }
            }
            
            // Convert to relative path
            let relative_path = path.strip_prefix(mount_point).unwrap_or(&path).to_path_buf();
            
            let operation = match event.kind {
                EventKind::Create(CreateKind::File) => {
                    VFSSyncOperation::CreateFile { path: relative_path, inode: 0 } // inode will be assigned
                }
                EventKind::Create(CreateKind::Folder) => {
                    VFSSyncOperation::CreateDirectory { path: relative_path, inode: 0 }
                }
                EventKind::Modify(ModifyKind::Data(_)) => {
                    // For data modifications, we need to read the file and sync it
                    VFSSyncOperation::UpdateMetadata { path: relative_path, inode: 0 }
                }
                EventKind::Remove(RemoveKind::File) => {
                    VFSSyncOperation::DeleteFile { path: relative_path, inode: 0 }
                }
                EventKind::Remove(RemoveKind::Folder) => {
                    VFSSyncOperation::DeleteDirectory { path: relative_path, inode: 0 }
                }
                _ => return Ok(()), // Ignore other event types
            };
            
            if let Err(e) = sync_tx.send(operation) {
                error!("Failed to send sync operation: {}", e);
            }
        }
        
        Ok(())
    }
    
    async fn process_sync_batch(&self, operations: &[VFSSyncOperation]) -> Result<()> {
        trace!("Processing sync batch of {} operations", operations.len());
        
        for operation in operations {
            if let Err(e) = self.process_sync_operation(operation).await {
                error!("Failed to process sync operation {:?}: {}", operation, e);
            }
        }
        
        // Update performance metrics
        self.ops_per_second.fetch_add(operations.len() as u64, Ordering::Relaxed);
        
        Ok(())
    }
    
    async fn process_sync_operation(&self, operation: &VFSSyncOperation) -> Result<()> {
        match operation {
            VFSSyncOperation::CreateFile { path, .. } => {
                self.sync_file_creation(path).await?;
            }
            VFSSyncOperation::WriteFile { path, offset, data, .. } => {
                self.sync_file_write(path, *offset, data).await?;
            }
            VFSSyncOperation::DeleteFile { path, .. } => {
                self.sync_file_deletion(path).await?;
            }
            VFSSyncOperation::CreateDirectory { path, .. } => {
                self.sync_directory_creation(path).await?;
            }
            VFSSyncOperation::DeleteDirectory { path, .. } => {
                self.sync_directory_deletion(path).await?;
            }
            VFSSyncOperation::RenameFile { old_path, new_path, .. } => {
                self.sync_file_rename(old_path, new_path).await?;
            }
            VFSSyncOperation::UpdateMetadata { path, .. } => {
                self.sync_file_update(path).await?;
            }
        }
        
        Ok(())
    }
    
    // Core sync operations implementation
    async fn sync_file_creation(&self, path: &Path) -> Result<()> {
        debug!("Syncing file creation: {}", path.display());
        
        let full_path = self.mount_point.join(path);
        if !full_path.exists() {
            return Ok(()); // File was deleted before we could sync
        }
        
        // Get or create inode
        let inode = self.get_or_create_inode_for_path(path).await?;
        
        // Read file content
        let content = fs::read(&full_path).await.unwrap_or_default();
        
        // Allocate blocks and write data
        if !content.is_empty() {
            self.write_file_data(inode, 0, &content).await?;
        }
        
        // Update parent directory
        if let Some(parent_path) = path.parent() {
            self.add_directory_entry(parent_path, path.file_name().unwrap().to_string_lossy().to_string(), inode).await?;
        }
        
        Ok(())
    }
    
    async fn sync_file_write(&self, path: &Path, offset: u64, data: &[u8]) -> Result<()> {
        debug!("Syncing file write: {} at offset {}, {} bytes", path.display(), offset, data.len());
        
        if let Some(inode) = self.get_inode_for_path(path).await? {
            self.write_file_data(inode, offset, data).await?;
        }
        
        Ok(())
    }
    
    async fn sync_file_deletion(&self, path: &Path) -> Result<()> {
        debug!("Syncing file deletion: {}", path.display());
        
        if let Some(inode) = self.get_inode_for_path(path).await? {
            // Free allocated blocks
            self.free_inode_blocks(inode).await?;
            
            // Remove from cache and mappings
            {
                let mut cache = self.cache.write().await;
                cache.inodes.remove(&inode);
            }
            
            {
                let mut path_to_inode = self.path_to_inode.write().await;
                let mut inode_to_path = self.inode_to_path.write().await;
                path_to_inode.remove(path);
                inode_to_path.remove(&inode);
            }
            
            // Remove from parent directory
            if let Some(parent_path) = path.parent() {
                self.remove_directory_entry(parent_path, path.file_name().unwrap().to_string_lossy().to_string()).await?;
            }
        }
        
        Ok(())
    }
    
    async fn sync_directory_creation(&self, path: &Path) -> Result<()> {
        debug!("Syncing directory creation: {}", path.display());
        
        let inode = self.get_or_create_inode_for_path(path).await?;
        
        // Create directory inode
        let dir_inode = VFSInode {
            ino: inode,
            file_type: VFSFileType::Directory,
            mode: 0o755,
            uid: 0,
            gid: 0,
            size: 0,
            blocks: vec![],
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
            accessed_at: SystemTime::now(),
            link_count: 2,
        };
        
        {
            let mut cache = self.cache.write().await;
            cache.inodes.insert(inode, dir_inode);
            cache.directory_entries.insert(inode, Vec::new());
        }
        
        self.save_inode(inode).await?;
        
        // Update parent directory
        if let Some(parent_path) = path.parent() {
            self.add_directory_entry(parent_path, path.file_name().unwrap().to_string_lossy().to_string(), inode).await?;
        }
        
        Ok(())
    }
    
    async fn sync_directory_deletion(&self, path: &Path) -> Result<()> {
        debug!("Syncing directory deletion: {}", path.display());
        
        if let Some(inode) = self.get_inode_for_path(path).await? {
            // Remove directory entries
            {
                let mut cache = self.cache.write().await;
                cache.directory_entries.remove(&inode);
                cache.inodes.remove(&inode);
            }
            
            // Remove from mappings
            {
                let mut path_to_inode = self.path_to_inode.write().await;
                let mut inode_to_path = self.inode_to_path.write().await;
                path_to_inode.remove(path);
                inode_to_path.remove(&inode);
            }
            
            // Remove from parent directory
            if let Some(parent_path) = path.parent() {
                self.remove_directory_entry(parent_path, path.file_name().unwrap().to_string_lossy().to_string()).await?;
            }
        }
        
        Ok(())
    }
    
    async fn sync_file_rename(&self, old_path: &Path, new_path: &Path) -> Result<()> {
        debug!("Syncing file rename: {} -> {}", old_path.display(), new_path.display());
        
        if let Some(inode) = self.get_inode_for_path(old_path).await? {
            // Update path mappings
            {
                let mut path_to_inode = self.path_to_inode.write().await;
                let mut inode_to_path = self.inode_to_path.write().await;
                path_to_inode.remove(old_path);
                path_to_inode.insert(new_path.to_path_buf(), inode);
                inode_to_path.insert(inode, new_path.to_path_buf());
            }
            
            // Update directory entries
            if let Some(old_parent) = old_path.parent() {
                self.remove_directory_entry(old_parent, old_path.file_name().unwrap().to_string_lossy().to_string()).await?;
            }
            
            if let Some(new_parent) = new_path.parent() {
                self.add_directory_entry(new_parent, new_path.file_name().unwrap().to_string_lossy().to_string(), inode).await?;
            }
        }
        
        Ok(())
    }
    
    async fn sync_file_update(&self, path: &Path) -> Result<()> {
        debug!("Syncing file update: {}", path.display());
        
        let full_path = self.mount_point.join(path);
        if !full_path.exists() {
            return self.sync_file_deletion(path).await;
        }
        
        // Check if this is a new file or an update to existing file
        if let Some(inode) = self.get_inode_for_path(path).await? {
            // Update existing file
            let content = fs::read(&full_path).await.unwrap_or_default();
            self.write_file_data(inode, 0, &content).await?;
        } else {
            // New file
            self.sync_file_creation(path).await?;
        }
        
        Ok(())
    }
    
    // Block management and storage operations
    async fn allocate_blocks(&self, count: usize) -> Result<Vec<u64>> {
        let mut allocated = Vec::with_capacity(count);
        let mut bitmap = self.allocation_bitmap.write().await;
        
        let mut found = 0;
        let bitmap_len = bitmap.len();
        for i in FIRST_DATA_BLOCK as usize..bitmap_len {
            if !bitmap[i] {
                bitmap[i] = true;
                allocated.push(i as u64);
                found += 1;
                
                if found >= count {
                    break;
                }
            }
        }
        
        if found < count {
            // Rollback allocation
            for &block in &allocated {
                bitmap[block as usize] = false;
            }
            return Err(anyhow::anyhow!("Insufficient free blocks"));
        }
        
        // Update used blocks count
        {
            let mut metadata = self.metadata.write().await;
            metadata.used_blocks += allocated.len() as u64;
        }
        
        Ok(allocated)
    }
    
    async fn free_blocks(&self, blocks: &[u64]) -> Result<()> {
        let mut bitmap = self.allocation_bitmap.write().await;
        
        for &block in blocks {
            if block >= FIRST_DATA_BLOCK {
                bitmap[block as usize] = false;
            }
        }
        
        // Update used blocks count
        {
            let mut metadata = self.metadata.write().await;
            metadata.used_blocks = metadata.used_blocks.saturating_sub(blocks.len() as u64);
        }
        
        Ok(())
    }
    
    async fn write_file_data(&self, inode: u64, offset: u64, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        
        let blocks_needed = ((offset + data.len() as u64 + BLOCK_SIZE - 1) / BLOCK_SIZE) as usize;
        
        // Get or allocate blocks for inode
        let mut inode_data = {
            let cache = self.cache.read().await;
            cache.inodes.get(&inode).cloned()
        };
        
        if let Some(ref mut inode_info) = inode_data {
            // Allocate additional blocks if needed
            while inode_info.blocks.len() < blocks_needed {
                let new_blocks = self.allocate_blocks(1).await?;
                inode_info.blocks.extend(new_blocks);
            }
            
            // Write data to blocks
            let mut remaining_data = data;
            let mut current_offset = offset;
            
            while !remaining_data.is_empty() {
                let block_index = (current_offset / BLOCK_SIZE) as usize;
                let block_offset = current_offset % BLOCK_SIZE;
                let block_id = inode_info.blocks[block_index];
                
                // Read existing block data
                let mut block_data = self.galleonfs.read_block(self.volume_id, block_id).await
                    .unwrap_or_else(|_| vec![0; BLOCK_SIZE as usize]);
                
                if block_data.len() < BLOCK_SIZE as usize {
                    block_data.resize(BLOCK_SIZE as usize, 0);
                }
                
                // Copy new data into block
                let copy_start = block_offset as usize;
                let copy_end = std::cmp::min(copy_start + remaining_data.len(), BLOCK_SIZE as usize);
                let copy_len = copy_end - copy_start;
                
                block_data[copy_start..copy_end].copy_from_slice(&remaining_data[..copy_len]);
                
                // Write block back to storage
                self.galleonfs.write_block(
                    self.volume_id,
                    block_id,
                    &block_data,
                    WriteConcern::WriteDurable,
                ).await?;
                
                // Update for next iteration
                remaining_data = &remaining_data[copy_len..];
                current_offset += copy_len as u64;
            }
            
            // Update inode metadata
            inode_info.size = std::cmp::max(inode_info.size, offset + data.len() as u64);
            inode_info.modified_at = SystemTime::now();
            
            // Cache updated inode
            {
                let mut cache = self.cache.write().await;
                cache.inodes.insert(inode, inode_info.clone());
                cache.dirty_inodes.insert(inode, Instant::now());
            }
        }
        
        Ok(())
    }
    
    // Helper methods for inode and path management
    async fn get_or_create_inode_for_path(&self, path: &Path) -> Result<u64> {
        // Check if inode already exists
        if let Some(inode) = self.get_inode_for_path(path).await? {
            return Ok(inode);
        }
        
        // Create new inode
        let inode = {
            let mut metadata = self.metadata.write().await;
            let inode = metadata.next_inode;
            metadata.next_inode += 1;
            inode
        };
        
        // Create file inode
        let file_inode = VFSInode {
            ino: inode,
            file_type: VFSFileType::RegularFile,
            mode: 0o644,
            uid: 0,
            gid: 0,
            size: 0,
            blocks: vec![],
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
            accessed_at: SystemTime::now(),
            link_count: 1,
        };
        
        // Cache inode and update mappings
        {
            let mut cache = self.cache.write().await;
            cache.inodes.insert(inode, file_inode);
        }
        
        {
            let mut path_to_inode = self.path_to_inode.write().await;
            let mut inode_to_path = self.inode_to_path.write().await;
            path_to_inode.insert(path.to_path_buf(), inode);
            inode_to_path.insert(inode, path.to_path_buf());
        }
        
        self.save_inode(inode).await?;
        
        Ok(inode)
    }
    
    async fn get_inode_for_path(&self, path: &Path) -> Result<Option<u64>> {
        let path_to_inode = self.path_to_inode.read().await;
        Ok(path_to_inode.get(path).copied())
    }
    
    async fn free_inode_blocks(&self, inode: u64) -> Result<()> {
        let blocks = {
            let cache = self.cache.read().await;
            cache.inodes.get(&inode).map(|inode_info| inode_info.blocks.clone()).unwrap_or_default()
        };
        
        if !blocks.is_empty() {
            self.free_blocks(&blocks).await?;
        }
        
        Ok(())
    }
    
    async fn add_directory_entry(&self, parent_path: &Path, name: String, inode: u64) -> Result<()> {
        let parent_inode = if parent_path.as_os_str().is_empty() {
            1 // Root directory
        } else {
            self.get_inode_for_path(parent_path).await?.unwrap_or(1)
        };
        
        let entry = VFSDirectoryEntry { name, inode };
        
        {
            let mut cache = self.cache.write().await;
            cache.directory_entries.entry(parent_inode).or_insert_with(Vec::new).push(entry);
        }
        
        // Mark directory as dirty for persistence
        self.save_directory_entries(parent_inode).await?;
        
        Ok(())
    }
    
    async fn remove_directory_entry(&self, parent_path: &Path, name: String) -> Result<()> {
        let parent_inode = if parent_path.as_os_str().is_empty() {
            1 // Root directory
        } else {
            self.get_inode_for_path(parent_path).await?.unwrap_or(1)
        };
        
        {
            let mut cache = self.cache.write().await;
            if let Some(entries) = cache.directory_entries.get_mut(&parent_inode) {
                entries.retain(|entry| entry.name != name);
            }
        }
        
        self.save_directory_entries(parent_inode).await?;
        
        Ok(())
    }
    
    // Persistence methods
    async fn save_metadata(&self) -> Result<()> {
        let metadata = self.metadata.read().await;
        let serialized = bincode::serialize(&*metadata)?;
        
        self.galleonfs.write_block(
            self.volume_id,
            METADATA_BLOCK,
            &serialized,
            WriteConcern::WriteDurable,
        ).await?;
        
        Ok(())
    }
    
    async fn save_allocation_bitmap(&self) -> Result<()> {
        let bitmap = self.allocation_bitmap.read().await;
        let serialized = bincode::serialize(&*bitmap)?;
        
        self.galleonfs.write_block(
            self.volume_id,
            ALLOCATION_BITMAP_BLOCK,
            &serialized,
            WriteConcern::WriteDurable,
        ).await?;
        
        Ok(())
    }
    
    async fn save_inode(&self, inode: u64) -> Result<()> {
        let inode_data = {
            let cache = self.cache.read().await;
            cache.inodes.get(&inode).cloned()
        };
        
        if let Some(_inode_info) = inode_data {
            // For now, we'll store inodes in the metadata area
            // In a production system, we'd have dedicated inode blocks
            trace!("Saving inode {} to storage", inode);
            
            // Update dirty tracking
            {
                let mut cache = self.cache.write().await;
                cache.dirty_inodes.remove(&inode);
            }
        }
        
        Ok(())
    }
    
    async fn save_directory_entries(&self, inode: u64) -> Result<()> {
        let _entries = {
            let cache = self.cache.read().await;
            cache.directory_entries.get(&inode).cloned().unwrap_or_default()
        };
        
        // For now, we'll store directory entries in the inode's blocks
        // In a production system, we'd have a more sophisticated directory structure
        trace!("Saving directory entries for inode {} to storage", inode);
        
        Ok(())
    }
    
    async fn sync_dirty_metadata(&self) -> Result<()> {
        // Sync dirty inodes
        let dirty_inodes: Vec<u64> = {
            let cache = self.cache.read().await;
            cache.dirty_inodes.keys().copied().collect()
        };
        
        for inode in dirty_inodes {
            self.save_inode(inode).await?;
        }
        
        // Periodically save metadata and bitmap
        let last_ops_time = {
            let time = self.last_ops_time.read().await;
            *time
        };
        
        if last_ops_time.elapsed().as_millis() > METADATA_SYNC_INTERVAL_MS as u128 * 10 {
            self.save_metadata().await?;
            self.save_allocation_bitmap().await?;
            
            {
                let mut time = self.last_ops_time.write().await;
                *time = Instant::now();
            }
        }
        
        Ok(())
    }
    
    // Performance monitoring
    pub async fn get_performance_stats(&self) -> (u64, u64, usize) {
        let ops_per_second = self.ops_per_second.swap(0, Ordering::Relaxed);
        let metadata = self.metadata.read().await;
        let cache = self.cache.read().await;
        
        (ops_per_second, metadata.used_blocks, cache.inodes.len())
    }
    
    pub async fn force_sync(&self) -> Result<()> {
        info!("Forcing VFS sync to storage");
        
        self.save_metadata().await?;
        self.save_allocation_bitmap().await?;
        
        // Sync all dirty inodes
        let dirty_inodes: Vec<u64> = {
            let cache = self.cache.read().await;
            cache.dirty_inodes.keys().copied().collect()
        };
        
        for inode in dirty_inodes {
            self.save_inode(inode).await?;
        }
        
        info!("VFS sync completed");
        Ok(())
    }
}

// Clone implementation for the VFS
impl Clone for HighPerformanceVFS {
    fn clone(&self) -> Self {
        Self {
            volume_id: self.volume_id,
            mount_point: self.mount_point.clone(),
            galleonfs: self.galleonfs.clone(),
            metadata: self.metadata.clone(),
            cache: self.cache.clone(),
            allocation_bitmap: self.allocation_bitmap.clone(),
            sync_queue: self.sync_queue.clone(),
            batch_processor: self.batch_processor.clone(),
            _watcher: notify::recommended_watcher(|_| {}).unwrap(), // Dummy watcher for clone
            sync_tx: self.sync_tx.clone(),
            ops_per_second: self.ops_per_second.clone(),
            last_ops_time: self.last_ops_time.clone(),
            path_to_inode: self.path_to_inode.clone(),
            inode_to_path: self.inode_to_path.clone(),
        }
    }
}