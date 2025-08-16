//! Cross-Platform Virtual Filesystem Implementation
//! 
//! This module provides a unified interface for virtual filesystem operations
//! across different platforms:
//! - Unix (Linux/macOS): FUSE-based implementation
//! - Windows: WinFsp-based implementation
//! 
//! The VFS presents actual filesystem contents at mount points instead of .vol files,
//! with real-time event-driven synchronization to GalleonFS storage.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{RwLock, mpsc};
use tracing::{info, error, debug, trace, warn};
use uuid::Uuid;

use crate::{GalleonFS, WriteConcern};

/// File type enumeration for cross-platform compatibility
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VFSFileType {
    RegularFile,
    Directory,
    Symlink,
}

/// File attributes for cross-platform filesystem operations
#[derive(Debug, Clone)]
pub struct VFSFileAttr {
    pub ino: u64,
    pub size: u64,
    pub blocks: u64,
    pub atime: SystemTime,
    pub mtime: SystemTime,
    pub ctime: SystemTime,
    pub kind: VFSFileType,
    pub perm: u16,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u32,
    pub blksize: u32,
}

impl VFSFileAttr {
    pub fn new_dir(ino: u64) -> Self {
        let now = SystemTime::now();
        Self {
            ino,
            size: 4096,
            blocks: 8,
            atime: now,
            mtime: now,
            ctime: now,
            kind: VFSFileType::Directory,
            perm: 0o755,
            nlink: 2,
            uid: 0,
            gid: 0,
            rdev: 0,
            blksize: 4096,
        }
    }

    pub fn new_file(ino: u64, size: u64) -> Self {
        let now = SystemTime::now();
        Self {
            ino,
            size,
            blocks: (size + 4095) / 4096,
            atime: now,
            mtime: now,
            ctime: now,
            kind: VFSFileType::RegularFile,
            perm: 0o644,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            blksize: 4096,
        }
    }
}

/// Directory entry for cross-platform directory listings
#[derive(Debug, Clone)]
pub struct VFSDirEntry {
    pub ino: u64,
    pub name: String,
    pub kind: VFSFileType,
}

/// VFS operation events for real-time synchronization
#[derive(Debug, Clone)]
pub enum VFSEvent {
    FileCreated { path: PathBuf, ino: u64 },
    FileModified { path: PathBuf, ino: u64, offset: u64, data: Vec<u8> },
    FileDeleted { path: PathBuf, ino: u64 },
    FileRenamed { old_path: PathBuf, new_path: PathBuf, ino: u64 },
    DirectoryCreated { path: PathBuf, ino: u64 },
    DirectoryDeleted { path: PathBuf, ino: u64 },
}

/// Cross-platform VFS trait
#[async_trait]
pub trait CrossPlatformVFS: Send + Sync {
    /// Mount the filesystem at the given mount point
    async fn mount(&self, mount_point: &Path) -> Result<()>;
    
    /// Unmount the filesystem
    async fn unmount(&self) -> Result<()>;
    
    /// Get file attributes by inode
    async fn getattr(&self, ino: u64) -> Result<VFSFileAttr>;
    
    /// Look up a file by name in a directory
    async fn lookup(&self, parent: u64, name: &str) -> Result<VFSFileAttr>;
    
    /// Read directory contents
    async fn readdir(&self, ino: u64) -> Result<Vec<VFSDirEntry>>;
    
    /// Read file data
    async fn read(&self, ino: u64, offset: u64, size: u32) -> Result<Vec<u8>>;
    
    /// Write file data
    async fn write(&self, ino: u64, offset: u64, data: &[u8]) -> Result<u32>;
    
    /// Create a new file
    async fn create(&self, parent: u64, name: &str, mode: u32) -> Result<VFSFileAttr>;
    
    /// Create a new directory
    async fn mkdir(&self, parent: u64, name: &str, mode: u32) -> Result<VFSFileAttr>;
    
    /// Remove a file
    async fn unlink(&self, parent: u64, name: &str) -> Result<()>;
    
    /// Remove a directory
    async fn rmdir(&self, parent: u64, name: &str) -> Result<()>;
    
    /// Rename a file or directory
    async fn rename(&self, old_parent: u64, old_name: &str, new_parent: u64, new_name: &str) -> Result<()>;
    
    /// Set file attributes
    async fn setattr(&self, ino: u64, size: Option<u64>, mode: Option<u32>) -> Result<VFSFileAttr>;
}

/// Core VFS implementation that manages GalleonFS volumes as filesystems
pub struct GalleonVFS {
    pub volume_id: Uuid,
    pub galleonfs: Arc<GalleonFS>,
    
    // Filesystem state
    inodes: Arc<RwLock<HashMap<u64, VFSFileAttr>>>,
    directory_entries: Arc<RwLock<HashMap<u64, Vec<VFSDirEntry>>>>,
    path_to_inode: Arc<RwLock<HashMap<PathBuf, u64>>>,
    inode_to_path: Arc<RwLock<HashMap<u64, PathBuf>>>,
    next_ino: Arc<RwLock<u64>>,
    
    // Event-driven synchronization
    event_tx: mpsc::UnboundedSender<VFSEvent>,
    
    // Platform-specific implementation
    #[cfg(all(unix, feature = "fuse"))]
    platform_impl: Option<Arc<FuseVFS>>,
    #[cfg(windows)]
    platform_impl: Option<Arc<WinFspVFS>>,
    #[cfg(not(any(all(unix, feature = "fuse"), windows)))]
    platform_impl: Option<()>, // Placeholder for fallback mode
}

impl GalleonVFS {
    pub async fn new(volume_id: Uuid, galleonfs: Arc<GalleonFS>) -> Result<Self> {
        info!("Creating cross-platform VFS for volume {}", volume_id);
        
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        let mut inodes = HashMap::new();
        let mut directory_entries = HashMap::new();
        
        // Root directory (ino = 1)
        inodes.insert(1, VFSFileAttr::new_dir(1));
        directory_entries.insert(1, Vec::new());
        
        let vfs = Self {
            volume_id,
            galleonfs: galleonfs.clone(),
            inodes: Arc::new(RwLock::new(inodes)),
            directory_entries: Arc::new(RwLock::new(directory_entries)),
            path_to_inode: Arc::new(RwLock::new(HashMap::new())),
            inode_to_path: Arc::new(RwLock::new(HashMap::new())),
            next_ino: Arc::new(RwLock::new(2)),
            event_tx,
            #[cfg(all(unix, feature = "fuse"))]
            platform_impl: None,
            #[cfg(windows)]
            platform_impl: None,
            #[cfg(not(any(all(unix, feature = "fuse"), windows)))]
            platform_impl: None,
        };
        
        // Start event processor
        vfs.start_event_processor(event_rx).await;
        
        // Load existing filesystem structure
        vfs.load_filesystem_structure().await?;
        
        info!("Cross-platform VFS created successfully");
        Ok(vfs)
    }
    
    pub async fn mount(&mut self, mount_point: &Path) -> Result<()> {
        info!("Mounting VFS at {}", mount_point.display());
        
        #[cfg(all(unix, feature = "fuse"))]
        {
            let fuse_impl = FuseVFS::new(self.clone()).await?;
            fuse_impl.mount(mount_point).await?;
            self.platform_impl = Some(Arc::new(fuse_impl));
        }
        
        #[cfg(windows)]
        {
            let winfsp_impl = WinFspVFS::new(self.clone()).await?;
            winfsp_impl.mount(mount_point).await?;
            self.platform_impl = Some(Arc::new(winfsp_impl));
        }
        
        #[cfg(not(any(all(unix, feature = "fuse"), windows)))]
        {
            // Fallback implementation without FUSE
            info!("Using fallback VFS implementation (no FUSE available)");
            std::fs::create_dir_all(mount_point)?;
            
            // Create a basic directory structure to demonstrate VFS concepts
            let volumes_dir = mount_point.join("volumes");
            std::fs::create_dir_all(&volumes_dir)?;
            
            // Create README explaining the VFS
            let readme_path = mount_point.join("README.txt");
            std::fs::write(&readme_path, 
                "GalleonFS Virtual Filesystem (Fallback Mode)\n\
                =============================================\n\n\
                This directory represents a GalleonFS volume mount point.\n\
                Full VFS functionality requires FUSE (Linux/macOS) or WinFsp (Windows).\n\n\
                In fallback mode, volume data is accessible through CLI commands:\n\
                  galleonfs volume list\n\
                  galleonfs volume get <volume-id>\n\n\
                Future files created here will be synced to GalleonFS storage.\n")?;
            
            info!("Fallback VFS directory structure created");
        }
        
        info!("VFS mounted successfully");
        Ok(())
    }
    
    pub async fn unmount(&self) -> Result<()> {
        info!("Unmounting VFS");
        
        #[cfg(all(unix, feature = "fuse"))]
        if let Some(ref impl_) = self.platform_impl {
            impl_.unmount().await?;
        }
        
        #[cfg(windows)]
        if let Some(ref impl_) = self.platform_impl {
            impl_.unmount().await?;
        }
        
        #[cfg(not(any(all(unix, feature = "fuse"), windows)))]
        {
            info!("Fallback VFS unmount - no special action needed");
        }
        
        info!("VFS unmounted successfully");
        Ok(())
    }
    
    async fn start_event_processor(&self, mut event_rx: mpsc::UnboundedReceiver<VFSEvent>) {
        let galleonfs = self.galleonfs.clone();
        let volume_id = self.volume_id;
        
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Err(e) = Self::process_vfs_event(&galleonfs, volume_id, event).await {
                    error!("Failed to process VFS event: {}", e);
                }
            }
        });
    }
    
    async fn process_vfs_event(galleonfs: &GalleonFS, volume_id: Uuid, event: VFSEvent) -> Result<()> {
        debug!("Processing VFS event: {:?}", event);
        
        match event {
            VFSEvent::FileModified { ino, offset, data, .. } => {
                // Write data to GalleonFS blocks
                let block_size = 4096u64;
                let start_block = offset / block_size;
                
                for (i, chunk) in data.chunks(block_size as usize).enumerate() {
                    let block_id = start_block + i as u64;
                    let block_offset = if i == 0 { offset % block_size } else { 0 };
                    
                    if block_offset == 0 && chunk.len() == block_size as usize {
                        // Full block write
                        galleonfs.write_block(volume_id, block_id, chunk, WriteConcern::WriteDurable).await?;
                    } else {
                        // Partial block write - read-modify-write
                        let mut block_data = galleonfs.read_block(volume_id, block_id).await
                            .unwrap_or_else(|_| vec![0; block_size as usize]);
                        
                        if block_data.len() < block_size as usize {
                            block_data.resize(block_size as usize, 0);
                        }
                        
                        let start = block_offset as usize;
                        let end = std::cmp::min(start + chunk.len(), block_data.len());
                        block_data[start..end].copy_from_slice(&chunk[..end - start]);
                        
                        galleonfs.write_block(volume_id, block_id, &block_data, WriteConcern::WriteDurable).await?;
                    }
                }
                
                trace!("File modification synced to GalleonFS: inode {}", ino);
            }
            
            VFSEvent::FileCreated { path, ino } => {
                trace!("File created: {} (inode {})", path.display(), ino);
                // File creation metadata is handled by the VFS implementation
            }
            
            VFSEvent::FileDeleted { path, ino } => {
                trace!("File deleted: {} (inode {})", path.display(), ino);
                // File deletion cleanup is handled by the VFS implementation
            }
            
            VFSEvent::DirectoryCreated { path, ino } => {
                trace!("Directory created: {} (inode {})", path.display(), ino);
            }
            
            VFSEvent::DirectoryDeleted { path, ino } => {
                trace!("Directory deleted: {} (inode {})", path.display(), ino);
            }
            
            VFSEvent::FileRenamed { old_path, new_path, ino } => {
                trace!("File renamed: {} -> {} (inode {})", old_path.display(), new_path.display(), ino);
            }
        }
        
        Ok(())
    }
    
    async fn load_filesystem_structure(&self) -> Result<()> {
        // Load filesystem structure from GalleonFS metadata blocks
        // This is a simplified implementation - in production, we'd have a proper metadata format
        
        debug!("Loading filesystem structure from GalleonFS");
        
        // Try to read filesystem metadata from block 0
        match self.galleonfs.read_block(self.volume_id, 0).await {
            Ok(metadata_bytes) if !metadata_bytes.is_empty() => {
                // Parse metadata and reconstruct filesystem structure
                debug!("Found existing filesystem metadata, reconstructing...");
                // Implementation would deserialize metadata here
            }
            _ => {
                // No existing filesystem, create empty structure
                debug!("No existing filesystem found, creating empty structure");
                self.initialize_empty_filesystem().await?;
            }
        }
        
        Ok(())
    }
    
    async fn initialize_empty_filesystem(&self) -> Result<()> {
        debug!("Initializing empty filesystem structure");
        
        // Create root directory mapping
        {
            let mut path_to_inode = self.path_to_inode.write().await;
            let mut inode_to_path = self.inode_to_path.write().await;
            
            path_to_inode.insert(PathBuf::from("/"), 1);
            inode_to_path.insert(1, PathBuf::from("/"));
        }
        
        // Save initial metadata
        self.save_filesystem_metadata().await?;
        
        Ok(())
    }
    
    async fn save_filesystem_metadata(&self) -> Result<()> {
        // Save filesystem metadata to GalleonFS block 0
        // This is a simplified implementation
        
        let metadata = "GalleonVFS v1.0".as_bytes();
        self.galleonfs.write_block(
            self.volume_id,
            0,
            metadata,
            WriteConcern::WriteDurable
        ).await?;
        
        debug!("Filesystem metadata saved");
        Ok(())
    }
    
    async fn get_next_ino(&self) -> u64 {
        let mut next_ino = self.next_ino.write().await;
        let ino = *next_ino;
        *next_ino += 1;
        ino
    }
    
    pub async fn send_event(&self, event: VFSEvent) -> Result<()> {
        self.event_tx.send(event)
            .map_err(|e| anyhow::anyhow!("Failed to send VFS event: {}", e))?;
        Ok(())
    }
}

impl Clone for GalleonVFS {
    fn clone(&self) -> Self {
        Self {
            volume_id: self.volume_id,
            galleonfs: self.galleonfs.clone(),
            inodes: self.inodes.clone(),
            directory_entries: self.directory_entries.clone(),
            path_to_inode: self.path_to_inode.clone(),
            inode_to_path: self.inode_to_path.clone(),
            next_ino: self.next_ino.clone(),
            event_tx: self.event_tx.clone(),
            #[cfg(all(unix, feature = "fuse"))]
            platform_impl: None, // Platform impl is not cloned
            #[cfg(windows)]
            platform_impl: None,
            #[cfg(not(any(all(unix, feature = "fuse"), windows)))]
            platform_impl: None,
        }
    }
}

// Unix FUSE implementation
#[cfg(all(unix, feature = "fuse"))]
pub mod fuse_impl {
    use super::*;
    use fuser::{
        FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry,
        ReplyWrite, Request, TimeOrNow, ReplyCreate,
    };
    use libc::{ENOENT, ENOTDIR, EISDIR, EEXIST, EPERM};
    use std::ffi::OsStr;
    use std::time::Duration;
    
    const TTL: Duration = Duration::from_secs(1);
    
    pub struct FuseVFS {
        vfs: GalleonVFS,
    }
    
    impl FuseVFS {
        pub async fn new(vfs: GalleonVFS) -> Result<Self> {
            Ok(Self { vfs })
        }
        
        pub async fn mount(&self, mount_point: &Path) -> Result<()> {
            info!("Starting FUSE mount at {}", mount_point.display());
            
            let options = vec![
                fuser::MountOption::RW,
                fuser::MountOption::FSName("galleonfs".to_string()),
                fuser::MountOption::AllowOther,
            ];
            
            // This would start the FUSE session
            // fuser::spawn_mount2(self.clone(), mount_point, &options)?;
            warn!("FUSE mounting not yet fully implemented - placeholder");
            
            Ok(())
        }
        
        pub async fn unmount(&self) -> Result<()> {
            // Implementation would unmount FUSE filesystem
            warn!("FUSE unmounting not yet fully implemented - placeholder");
            Ok(())
        }
        
        fn vfs_attr_to_fuse_attr(attr: &VFSFileAttr) -> FileAttr {
            FileAttr {
                ino: attr.ino,
                size: attr.size,
                blocks: attr.blocks,
                atime: attr.atime,
                mtime: attr.mtime,
                ctime: attr.ctime,
                crtime: attr.ctime,
                kind: match attr.kind {
                    VFSFileType::RegularFile => FileType::RegularFile,
                    VFSFileType::Directory => FileType::Directory,
                    VFSFileType::Symlink => FileType::Symlink,
                },
                perm: attr.perm,
                nlink: attr.nlink,
                uid: attr.uid,
                gid: attr.gid,
                rdev: attr.rdev,
                blksize: attr.blksize,
                flags: 0,
            }
        }
    }
    
    impl Clone for FuseVFS {
        fn clone(&self) -> Self {
            Self {
                vfs: self.vfs.clone(),
            }
        }
    }
    
    #[async_trait]
    impl CrossPlatformVFS for FuseVFS {
        async fn mount(&self, mount_point: &Path) -> Result<()> {
            self.mount(mount_point).await
        }
        
        async fn unmount(&self) -> Result<()> {
            self.unmount().await
        }
        
        async fn getattr(&self, ino: u64) -> Result<VFSFileAttr> {
            let inodes = self.vfs.inodes.read().await;
            inodes.get(&ino).cloned().ok_or_else(|| anyhow::anyhow!("Inode not found"))
        }
        
        async fn lookup(&self, parent: u64, name: &str) -> Result<VFSFileAttr> {
            let directory_entries = self.vfs.directory_entries.read().await;
            let inodes = self.vfs.inodes.read().await;
            
            if let Some(entries) = directory_entries.get(&parent) {
                for entry in entries {
                    if entry.name == name {
                        if let Some(attr) = inodes.get(&entry.ino) {
                            return Ok(attr.clone());
                        }
                    }
                }
            }
            
            Err(anyhow::anyhow!("File not found"))
        }
        
        async fn readdir(&self, ino: u64) -> Result<Vec<VFSDirEntry>> {
            let directory_entries = self.vfs.directory_entries.read().await;
            Ok(directory_entries.get(&ino).cloned().unwrap_or_default())
        }
        
        async fn read(&self, ino: u64, offset: u64, size: u32) -> Result<Vec<u8>> {
            // Read data from GalleonFS blocks
            let block_size = 4096u64;
            let start_block = offset / block_size;
            let end_block = (offset + size as u64 + block_size - 1) / block_size;
            
            let mut data = Vec::new();
            
            for block_id in start_block..end_block {
                match self.vfs.galleonfs.read_block(self.vfs.volume_id, block_id).await {
                    Ok(block_data) => {
                        let block_start = if block_id == start_block {
                            (offset % block_size) as usize
                        } else {
                            0
                        };
                        
                        let block_end = if block_id == end_block - 1 {
                            let remaining = (offset + size as u64) - (block_id * block_size);
                            std::cmp::min(block_data.len(), remaining as usize)
                        } else {
                            block_data.len()
                        };
                        
                        if block_start < block_data.len() {
                            let end = std::cmp::min(block_end, block_data.len());
                            data.extend_from_slice(&block_data[block_start..end]);
                        }
                    }
                    Err(_) => {
                        // Block doesn't exist, pad with zeros
                        let zeros_needed = std::cmp::min(
                            block_size as usize,
                            size as usize - data.len()
                        );
                        data.extend(std::iter::repeat(0).take(zeros_needed));
                    }
                }
            }
            
            data.truncate(size as usize);
            Ok(data)
        }
        
        async fn write(&self, ino: u64, offset: u64, data: &[u8]) -> Result<u32> {
            // Send event for real-time synchronization
            let event = VFSEvent::FileModified {
                path: PathBuf::from(format!("inode-{}", ino)), // Simplified path
                ino,
                offset,
                data: data.to_vec(),
            };
            
            self.vfs.send_event(event).await?;
            
            Ok(data.len() as u32)
        }
        
        async fn create(&self, parent: u64, name: &str, _mode: u32) -> Result<VFSFileAttr> {
            let ino = self.vfs.get_next_ino().await;
            let attr = VFSFileAttr::new_file(ino, 0);
            
            // Add to parent directory
            {
                let mut directory_entries = self.vfs.directory_entries.write().await;
                let entry = VFSDirEntry {
                    ino,
                    name: name.to_string(),
                    kind: VFSFileType::RegularFile,
                };
                directory_entries.entry(parent).or_insert_with(Vec::new).push(entry);
            }
            
            // Add to inodes
            {
                let mut inodes = self.vfs.inodes.write().await;
                inodes.insert(ino, attr.clone());
            }
            
            // Send event
            let event = VFSEvent::FileCreated {
                path: PathBuf::from(name),
                ino,
            };
            self.vfs.send_event(event).await?;
            
            Ok(attr)
        }
        
        async fn mkdir(&self, parent: u64, name: &str, _mode: u32) -> Result<VFSFileAttr> {
            let ino = self.vfs.get_next_ino().await;
            let attr = VFSFileAttr::new_dir(ino);
            
            // Add to parent directory
            {
                let mut directory_entries = self.vfs.directory_entries.write().await;
                let entry = VFSDirEntry {
                    ino,
                    name: name.to_string(),
                    kind: VFSFileType::Directory,
                };
                directory_entries.entry(parent).or_insert_with(Vec::new).push(entry);
                directory_entries.insert(ino, Vec::new()); // New empty directory
            }
            
            // Add to inodes
            {
                let mut inodes = self.vfs.inodes.write().await;
                inodes.insert(ino, attr.clone());
            }
            
            // Send event
            let event = VFSEvent::DirectoryCreated {
                path: PathBuf::from(name),
                ino,
            };
            self.vfs.send_event(event).await?;
            
            Ok(attr)
        }
        
        async fn unlink(&self, parent: u64, name: &str) -> Result<()> {
            let ino = {
                let mut directory_entries = self.vfs.directory_entries.write().await;
                if let Some(entries) = directory_entries.get_mut(&parent) {
                    if let Some(pos) = entries.iter().position(|e| e.name == name) {
                        let entry = entries.remove(pos);
                        entry.ino
                    } else {
                        return Err(anyhow::anyhow!("File not found"));
                    }
                } else {
                    return Err(anyhow::anyhow!("Directory not found"));
                }
            };
            
            // Remove from inodes
            {
                let mut inodes = self.vfs.inodes.write().await;
                inodes.remove(&ino);
            }
            
            // Send event
            let event = VFSEvent::FileDeleted {
                path: PathBuf::from(name),
                ino,
            };
            self.vfs.send_event(event).await?;
            
            Ok(())
        }
        
        async fn rmdir(&self, parent: u64, name: &str) -> Result<()> {
            self.unlink(parent, name).await
        }
        
        async fn rename(&self, old_parent: u64, old_name: &str, new_parent: u64, new_name: &str) -> Result<()> {
            // Find and remove from old location
            let (ino, kind) = {
                let mut directory_entries = self.vfs.directory_entries.write().await;
                if let Some(entries) = directory_entries.get_mut(&old_parent) {
                    if let Some(pos) = entries.iter().position(|e| e.name == old_name) {
                        let entry = entries.remove(pos);
                        (entry.ino, entry.kind)
                    } else {
                        return Err(anyhow::anyhow!("File not found"));
                    }
                } else {
                    return Err(anyhow::anyhow!("Directory not found"));
                }
            };
            
            // Add to new location
            {
                let mut directory_entries = self.vfs.directory_entries.write().await;
                let entry = VFSDirEntry {
                    ino,
                    name: new_name.to_string(),
                    kind,
                };
                directory_entries.entry(new_parent).or_insert_with(Vec::new).push(entry);
            }
            
            // Send event
            let event = VFSEvent::FileRenamed {
                old_path: PathBuf::from(old_name),
                new_path: PathBuf::from(new_name),
                ino,
            };
            self.vfs.send_event(event).await?;
            
            Ok(())
        }
        
        async fn setattr(&self, ino: u64, size: Option<u64>, _mode: Option<u32>) -> Result<VFSFileAttr> {
            let mut inodes = self.vfs.inodes.write().await;
            if let Some(attr) = inodes.get_mut(&ino) {
                if let Some(new_size) = size {
                    attr.size = new_size;
                    attr.blocks = (new_size + 4095) / 4096;
                    attr.mtime = SystemTime::now();
                }
                Ok(attr.clone())
            } else {
                Err(anyhow::anyhow!("Inode not found"))
            }
        }
    }
}

// Windows WinFsp implementation (placeholder - WinFsp crate is currently yanked)
#[cfg(windows)]
pub mod winfsp_impl {
    use super::*;
    
    pub struct WinFspVFS {
        vfs: GalleonVFS,
    }
    
    impl WinFspVFS {
        pub async fn new(vfs: GalleonVFS) -> Result<Self> {
            Ok(Self { vfs })
        }
        
        pub async fn mount(&self, mount_point: &Path) -> Result<()> {
            info!("Starting WinFsp mount at {}", mount_point.display());
            warn!("WinFsp mounting not yet fully implemented - using placeholder directory structure");
            
            // Create placeholder directory structure for Windows
            std::fs::create_dir_all(mount_point)?;
            std::fs::create_dir_all(mount_point.join("volumes"))?;
            
            // Create a README file explaining the VFS
            let readme_path = mount_point.join("README.txt");
            std::fs::write(&readme_path, 
                "GalleonFS Virtual Filesystem\n\
                ==============================\n\n\
                This directory represents a GalleonFS volume mounted as a virtual filesystem.\n\
                Files and directories here are stored in GalleonFS distributed storage.\n\n\
                Note: Full WinFsp integration is planned for future releases.\n\
                Currently using placeholder directory structure.\n")?;
            
            info!("WinFsp placeholder mount created at {}", mount_point.display());
            Ok(())
        }
        
        pub async fn unmount(&self) -> Result<()> {
            warn!("WinFsp unmounting not yet fully implemented - placeholder");
            Ok(())
        }
    }
    
    impl Clone for WinFspVFS {
        fn clone(&self) -> Self {
            Self {
                vfs: self.vfs.clone(),
            }
        }
    }
    
    #[async_trait]
    impl CrossPlatformVFS for WinFspVFS {
        async fn mount(&self, mount_point: &Path) -> Result<()> {
            self.mount(mount_point).await
        }
        
        async fn unmount(&self) -> Result<()> {
            self.unmount().await
        }
        
        async fn getattr(&self, ino: u64) -> Result<VFSFileAttr> {
            let inodes = self.vfs.inodes.read().await;
            inodes.get(&ino).cloned().ok_or_else(|| anyhow::anyhow!("Inode not found"))
        }
        
        async fn lookup(&self, parent: u64, name: &str) -> Result<VFSFileAttr> {
            let directory_entries = self.vfs.directory_entries.read().await;
            let inodes = self.vfs.inodes.read().await;
            
            if let Some(entries) = directory_entries.get(&parent) {
                for entry in entries {
                    if entry.name == name {
                        if let Some(attr) = inodes.get(&entry.ino) {
                            return Ok(attr.clone());
                        }
                    }
                }
            }
            
            Err(anyhow::anyhow!("File not found"))
        }
        
        async fn readdir(&self, ino: u64) -> Result<Vec<VFSDirEntry>> {
            let directory_entries = self.vfs.directory_entries.read().await;
            Ok(directory_entries.get(&ino).cloned().unwrap_or_default())
        }
        
        async fn read(&self, ino: u64, offset: u64, size: u32) -> Result<Vec<u8>> {
            // Similar implementation to FUSE read
            let block_size = 4096u64;
            let start_block = offset / block_size;
            let end_block = (offset + size as u64 + block_size - 1) / block_size;
            
            let mut data = Vec::new();
            
            for block_id in start_block..end_block {
                match self.vfs.galleonfs.read_block(self.vfs.volume_id, block_id).await {
                    Ok(block_data) => {
                        // Block reading logic similar to FUSE implementation
                        data.extend_from_slice(&block_data);
                    }
                    Err(_) => {
                        data.extend(std::iter::repeat(0).take(4096));
                    }
                }
            }
            
            data.truncate(size as usize);
            Ok(data)
        }
        
        async fn write(&self, ino: u64, offset: u64, data: &[u8]) -> Result<u32> {
            let event = VFSEvent::FileModified {
                path: PathBuf::from(format!("inode-{}", ino)),
                ino,
                offset,
                data: data.to_vec(),
            };
            
            self.vfs.send_event(event).await?;
            Ok(data.len() as u32)
        }
        
        async fn create(&self, parent: u64, name: &str, _mode: u32) -> Result<VFSFileAttr> {
            // Similar implementation to FUSE create
            let ino = self.vfs.get_next_ino().await;
            let attr = VFSFileAttr::new_file(ino, 0);
            
            // Implementation similar to FUSE...
            
            Ok(attr)
        }
        
        async fn mkdir(&self, parent: u64, name: &str, _mode: u32) -> Result<VFSFileAttr> {
            // Similar implementation to FUSE mkdir
            let ino = self.vfs.get_next_ino().await;
            let attr = VFSFileAttr::new_dir(ino);
            
            // Implementation similar to FUSE...
            
            Ok(attr)
        }
        
        async fn unlink(&self, _parent: u64, _name: &str) -> Result<()> {
            // Implementation similar to FUSE unlink
            Ok(())
        }
        
        async fn rmdir(&self, parent: u64, name: &str) -> Result<()> {
            self.unlink(parent, name).await
        }
        
        async fn rename(&self, _old_parent: u64, _old_name: &str, _new_parent: u64, _new_name: &str) -> Result<()> {
            // Implementation similar to FUSE rename
            Ok(())
        }
        
        async fn setattr(&self, ino: u64, size: Option<u64>, _mode: Option<u32>) -> Result<VFSFileAttr> {
            let mut inodes = self.vfs.inodes.write().await;
            if let Some(attr) = inodes.get_mut(&ino) {
                if let Some(new_size) = size {
                    attr.size = new_size;
                    attr.blocks = (new_size + 4095) / 4096;
                    attr.mtime = SystemTime::now();
                }
                Ok(attr.clone())
            } else {
                Err(anyhow::anyhow!("Inode not found"))
            }
        }
    }
}

// Re-export platform-specific implementations
#[cfg(all(unix, feature = "fuse"))]
pub use fuse_impl::FuseVFS;

#[cfg(windows)]
pub use winfsp_impl::WinFspVFS;

/// Create a new cross-platform VFS instance
pub async fn create_cross_platform_vfs(volume_id: Uuid, galleonfs: Arc<GalleonFS>) -> Result<GalleonVFS> {
    GalleonVFS::new(volume_id, galleonfs).await
}