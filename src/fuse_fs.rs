use anyhow::Result;
use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry,
    ReplyWrite, Request, TimeOrNow,
};
use libc::{c_int, ENOENT, ENOTDIR, EISDIR, EEXIST, EPERM};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{GalleonFS, Volume, VolumeState, WriteConcern};

const TTL: Duration = Duration::from_secs(1);
const BLOCK_SIZE: u64 = 4096;

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub ino: u64,
    pub name: String,
    pub kind: FileType,
    pub size: u64,
    pub blocks: u64,
    pub atime: SystemTime,
    pub mtime: SystemTime,
    pub ctime: SystemTime,
    pub mode: u16,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u32,
    pub blksize: u32,
    pub volume_id: Option<Uuid>,
    pub block_offset: u64,
}

impl FileMetadata {
    pub fn new_dir(ino: u64, name: String) -> Self {
        let now = SystemTime::now();
        Self {
            ino,
            name,
            kind: FileType::Directory,
            size: 4096,
            blocks: 8,
            atime: now,
            mtime: now,
            ctime: now,
            mode: 0o755,
            nlink: 2,
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
            rdev: 0,
            blksize: BLOCK_SIZE as u32,
            volume_id: None,
            block_offset: 0,
        }
    }

    pub fn new_file(ino: u64, name: String, size: u64, volume_id: Uuid) -> Self {
        let now = SystemTime::now();
        Self {
            ino,
            name,
            kind: FileType::RegularFile,
            size,
            blocks: (size + BLOCK_SIZE - 1) / BLOCK_SIZE,
            atime: now,
            mtime: now,
            ctime: now,
            mode: 0o644,
            nlink: 1,
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
            rdev: 0,
            blksize: BLOCK_SIZE as u32,
            volume_id: Some(volume_id),
            block_offset: 0,
        }
    }

    pub fn to_file_attr(&self) -> FileAttr {
        FileAttr {
            ino: self.ino,
            size: self.size,
            blocks: self.blocks,
            atime: self.atime,
            mtime: self.mtime,
            ctime: self.ctime,
            crtime: self.ctime,
            kind: self.kind,
            perm: self.mode,
            nlink: self.nlink,
            uid: self.uid,
            gid: self.gid,
            rdev: self.rdev,
            blksize: self.blksize,
            flags: 0,
        }
    }
}

pub struct GalleonFuseFS {
    galleonfs: Arc<GalleonFS>,
    inodes: Arc<RwLock<HashMap<u64, FileMetadata>>>,
    next_ino: Arc<RwLock<u64>>,
    volume_files: Arc<RwLock<HashMap<Uuid, u64>>>, // volume_id -> inode mapping
}

impl GalleonFuseFS {
    pub fn new(galleonfs: Arc<GalleonFS>) -> Self {
        let mut inodes = HashMap::new();
        
        // Root directory
        inodes.insert(1, FileMetadata::new_dir(1, "/".to_string()));
        
        // Volumes directory
        inodes.insert(2, FileMetadata::new_dir(2, "volumes".to_string()));
        
        Self {
            galleonfs,
            inodes: Arc::new(RwLock::new(inodes)),
            next_ino: Arc::new(RwLock::new(3)),
            volume_files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn get_next_ino(&self) -> u64 {
        let mut next_ino = self.next_ino.write().await;
        let ino = *next_ino;
        *next_ino += 1;
        ino
    }

    async fn sync_volumes(&self) -> Result<()> {
        let volumes = self.galleonfs.list_volumes().await?;
        let mut inodes = self.inodes.write().await;
        let mut volume_files = self.volume_files.write().await;

        // Add volume files that don't exist yet
        for volume in volumes {
            if !volume_files.contains_key(&volume.id) {
                let ino = {
                    let mut next_ino = self.next_ino.write().await;
                    let ino = *next_ino;
                    *next_ino += 1;
                    ino
                };

                let filename = format!("{}.vol", volume.name);
                let metadata = FileMetadata::new_file(ino, filename, volume.size_bytes, volume.id);
                
                inodes.insert(ino, metadata);
                volume_files.insert(volume.id, ino);
            }
        }

        Ok(())
    }

    async fn find_by_name(&self, parent: u64, name: &OsStr) -> Option<FileMetadata> {
        if let Err(_) = self.sync_volumes().await {
            return None;
        }

        let inodes = self.inodes.read().await;
        let name_str = name.to_string_lossy();

        if parent == 1 {
            // Root directory - only contains "volumes" subdirectory
            if name_str == "volumes" {
                return inodes.get(&2).cloned();
            }
        } else if parent == 2 {
            // Volumes directory - contains volume files
            for metadata in inodes.values() {
                if metadata.name == name_str && metadata.volume_id.is_some() {
                    return Some(metadata.clone());
                }
            }
        }

        None
    }

    async fn read_volume_data(&self, volume_id: Uuid, offset: u64, size: u32) -> Result<Vec<u8>> {
        let block_size = BLOCK_SIZE;
        let start_block = offset / block_size;
        let end_block = (offset + size as u64 + block_size - 1) / block_size;
        
        let mut data = Vec::new();
        
        for block_id in start_block..end_block {
            match self.galleonfs.read_block(volume_id, block_id).await {
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

    async fn write_volume_data(&self, volume_id: Uuid, offset: u64, data: &[u8]) -> Result<u32> {
        let block_size = BLOCK_SIZE;
        let start_block = offset / block_size;
        let mut written = 0;

        for (i, chunk) in data.chunks(block_size as usize).enumerate() {
            let block_id = start_block + i as u64;
            let block_offset = if i == 0 { offset % block_size } else { 0 };

            if block_offset == 0 && chunk.len() == block_size as usize {
                // Full block write
                self.galleonfs.write_block(
                    volume_id,
                    block_id,
                    chunk,
                    WriteConcern::WriteDurable
                ).await?;
                written += chunk.len() as u32;
            } else {
                // Partial block write - read-modify-write
                let mut block_data = match self.galleonfs.read_block(volume_id, block_id).await {
                    Ok(data) => data,
                    Err(_) => vec![0; block_size as usize],
                };

                // Ensure block is large enough
                if block_data.len() < block_size as usize {
                    block_data.resize(block_size as usize, 0);
                }

                // Copy new data into block
                let start = block_offset as usize;
                let end = std::cmp::min(start + chunk.len(), block_data.len());
                let copy_len = end - start;
                
                block_data[start..end].copy_from_slice(&chunk[..copy_len]);

                self.galleonfs.write_block(
                    volume_id,
                    block_id,
                    &block_data,
                    WriteConcern::WriteDurable
                ).await?;
                
                written += copy_len as u32;
            }
        }

        Ok(written)
    }
}

impl Filesystem for GalleonFuseFS {
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let galleonfs = self.clone();
        
        tokio::spawn(async move {
            match galleonfs.find_by_name(parent, name).await {
                Some(metadata) => {
                    reply.entry(&TTL, &metadata.to_file_attr(), 0);
                }
                None => {
                    reply.error(ENOENT);
                }
            }
        });
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyAttr) {
        let galleonfs = self.clone();
        
        tokio::spawn(async move {
            if let Err(_) = galleonfs.sync_volumes().await {
                reply.error(ENOENT);
                return;
            }

            let inodes = galleonfs.inodes.read().await;
            match inodes.get(&ino) {
                Some(metadata) => {
                    reply.attr(&TTL, &metadata.to_file_attr());
                }
                None => {
                    reply.error(ENOENT);
                }
            }
        });
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let galleonfs = self.clone();
        
        tokio::spawn(async move {
            if let Err(_) = galleonfs.sync_volumes().await {
                reply.error(ENOENT);
                return;
            }

            let inodes = galleonfs.inodes.read().await;
            
            match ino {
                1 => {
                    // Root directory
                    let entries = vec![
                        (1, FileType::Directory, "."),
                        (1, FileType::Directory, ".."),
                        (2, FileType::Directory, "volumes"),
                    ];

                    for (i, (ino, kind, name)) in entries.into_iter().enumerate().skip(offset as usize) {
                        if reply.add(ino, (i + 1) as i64, kind, name) {
                            break;
                        }
                    }
                    reply.ok();
                }
                2 => {
                    // Volumes directory
                    let mut entries = vec![
                        (2, FileType::Directory, ".".to_string()),
                        (1, FileType::Directory, "..".to_string()),
                    ];

                    // Add volume files
                    for metadata in inodes.values() {
                        if metadata.volume_id.is_some() {
                            entries.push((metadata.ino, FileType::RegularFile, metadata.name.clone()));
                        }
                    }

                    for (i, (ino, kind, name)) in entries.into_iter().enumerate().skip(offset as usize) {
                        if reply.add(ino, (i + 1) as i64, kind, &name) {
                            break;
                        }
                    }
                    reply.ok();
                }
                _ => {
                    // File - not a directory
                    reply.error(ENOTDIR);
                }
            }
        });
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyData,
    ) {
        let galleonfs = self.clone();
        
        tokio::spawn(async move {
            let inodes = galleonfs.inodes.read().await;
            
            match inodes.get(&ino) {
                Some(metadata) if metadata.volume_id.is_some() => {
                    let volume_id = metadata.volume_id.unwrap();
                    
                    match galleonfs.read_volume_data(volume_id, offset as u64, size).await {
                        Ok(data) => {
                            reply.data(&data);
                        }
                        Err(_) => {
                            reply.error(libc::EIO);
                        }
                    }
                }
                Some(_) => {
                    // Directory or other non-file
                    reply.error(EISDIR);
                }
                None => {
                    reply.error(ENOENT);
                }
            }
        });
    }

    fn write(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        let galleonfs = self.clone();
        let data = data.to_vec();
        
        tokio::spawn(async move {
            let mut inodes = galleonfs.inodes.write().await;
            
            match inodes.get_mut(&ino) {
                Some(metadata) if metadata.volume_id.is_some() => {
                    let volume_id = metadata.volume_id.unwrap();
                    
                    match galleonfs.write_volume_data(volume_id, offset as u64, &data).await {
                        Ok(written) => {
                            // Update file size if necessary
                            let new_size = std::cmp::max(
                                metadata.size, 
                                offset as u64 + written as u64
                            );
                            
                            if new_size != metadata.size {
                                metadata.size = new_size;
                                metadata.blocks = (new_size + BLOCK_SIZE - 1) / BLOCK_SIZE;
                                metadata.mtime = SystemTime::now();
                            }
                            
                            reply.written(written);
                        }
                        Err(_) => {
                            reply.error(libc::EIO);
                        }
                    }
                }
                Some(_) => {
                    // Directory or other non-file
                    reply.error(EISDIR);
                }
                None => {
                    reply.error(ENOENT);
                }
            }
        });
    }

    fn setattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let galleonfs = self.clone();
        
        tokio::spawn(async move {
            let mut inodes = galleonfs.inodes.write().await;
            
            match inodes.get_mut(&ino) {
                Some(metadata) => {
                    // Handle file truncation
                    if let Some(new_size) = size {
                        if metadata.volume_id.is_some() && new_size != metadata.size {
                            metadata.size = new_size;
                            metadata.blocks = (new_size + BLOCK_SIZE - 1) / BLOCK_SIZE;
                            metadata.mtime = SystemTime::now();
                            
                            // Note: For a complete implementation, we should actually
                            // truncate/extend the volume data here, but for now we just
                            // update the metadata
                        }
                    }
                    
                    reply.attr(&TTL, &metadata.to_file_attr());
                }
                None => {
                    reply.error(ENOENT);
                }
            }
        });
    }
}

impl Clone for GalleonFuseFS {
    fn clone(&self) -> Self {
        Self {
            galleonfs: self.galleonfs.clone(),
            inodes: self.inodes.clone(),
            next_ino: self.next_ino.clone(),
            volume_files: self.volume_files.clone(),
        }
    }
}