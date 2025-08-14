use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::fs;
use std::io::{Write, SeekFrom};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{GalleonFS, Volume, WriteConcern};

pub struct VirtualVolume {
    pub volume_id: Uuid,
    pub volume: Volume,
    pub file_path: PathBuf,
}

impl VirtualVolume {
    pub fn new(mount_point: &Path, volume: Volume) -> Self {
        let filename = format!("{}.vol", volume.name);
        let file_path = mount_point.join("volumes").join(filename);
        
        Self {
            volume_id: volume.id,
            volume,
            file_path,
        }
    }
    
    pub async fn read(&self, galleonfs: &GalleonFS, offset: u64, size: u64) -> Result<Vec<u8>> {
        let block_size = 4096u64;
        let start_block = offset / block_size;
        let end_block = (offset + size + block_size - 1) / block_size;
        
        let mut data = Vec::new();
        
        for block_id in start_block..end_block {
            match galleonfs.read_block(self.volume_id, block_id).await {
                Ok(block_data) => {
                    let block_start = if block_id == start_block {
                        (offset % block_size) as usize
                    } else {
                        0
                    };
                    
                    let block_end = if block_id == end_block - 1 {
                        let remaining = (offset + size) - (block_id * block_size);
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
                        (block_size - (offset % block_size)) as usize,
                        size as usize - data.len()
                    );
                    data.extend(std::iter::repeat(0).take(zeros_needed));
                }
            }
        }
        
        data.truncate(size as usize);
        Ok(data)
    }
    
    pub async fn write(&self, galleonfs: &GalleonFS, offset: u64, data: &[u8]) -> Result<usize> {
        let block_size = 4096u64;
        let start_block = offset / block_size;
        let mut written = 0;

        for (i, chunk) in data.chunks(block_size as usize).enumerate() {
            let block_id = start_block + i as u64;
            let block_offset = if i == 0 { offset % block_size } else { 0 };

            if block_offset == 0 && chunk.len() == block_size as usize {
                // Full block write
                galleonfs.write_block(
                    self.volume_id,
                    block_id,
                    chunk,
                    WriteConcern::WriteDurable
                ).await?;
                written += chunk.len();
            } else {
                // Partial block write - read-modify-write
                let mut block_data = match galleonfs.read_block(self.volume_id, block_id).await {
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

                galleonfs.write_block(
                    self.volume_id,
                    block_id,
                    &block_data,
                    WriteConcern::WriteDurable
                ).await?;
                
                written += copy_len;
            }
        }

        Ok(written)
    }
}

pub struct GalleonVirtualFS {
    galleonfs: Arc<GalleonFS>,
    mount_point: PathBuf,
    volumes: Arc<RwLock<HashMap<Uuid, VirtualVolume>>>,
}

impl GalleonVirtualFS {
    pub fn new(galleonfs: Arc<GalleonFS>, mount_point: PathBuf) -> Result<Self> {
        // Create directory structure
        fs::create_dir_all(&mount_point)?;
        fs::create_dir_all(mount_point.join("volumes"))?;
        
        // Create README
        let readme_path = mount_point.join("README.txt");
        let mut readme_file = fs::File::create(readme_path)?;
        writeln!(readme_file, "GalleonFS Virtual Mount Point")?;
        writeln!(readme_file, "=============================")?;
        writeln!(readme_file, "")?;
        writeln!(readme_file, "This directory contains GalleonFS volumes as virtual files.")?;
        writeln!(readme_file, "Each .vol file represents a GalleonFS volume that can be")?;
        writeln!(readme_file, "read from and written to using standard file operations.")?;
        writeln!(readme_file, "")?;
        writeln!(readme_file, "Usage examples:")?;
        writeln!(readme_file, "  - Read from a volume:  cat volumes/my-volume.vol")?;
        writeln!(readme_file, "  - Write to a volume:   echo 'data' > volumes/my-volume.vol")?;
        writeln!(readme_file, "  - Copy files:          cp myfile.txt volumes/my-volume.vol")?;
        writeln!(readme_file, "")?;
        writeln!(readme_file, "Note: This is a virtual filesystem. The .vol files are")?;
        writeln!(readme_file, "backed by GalleonFS storage, not regular disk files.")?;

        Ok(Self {
            galleonfs,
            mount_point,
            volumes: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    pub async fn sync_volumes(&self) -> Result<()> {
        let volumes_list = self.galleonfs.list_volumes().await?;
        let mut volumes = self.volumes.write().await;
        
        for volume in volumes_list {
            if !volumes.contains_key(&volume.id) {
                let virtual_vol = VirtualVolume::new(&self.mount_point, volume.clone());
                
                // Create a file handle for this volume
                self.create_volume_proxy(&virtual_vol).await?;
                
                volumes.insert(volume.id, virtual_vol);
            }
        }
        
        Ok(())
    }
    
    async fn create_volume_proxy(&self, virtual_vol: &VirtualVolume) -> Result<()> {
        // Create a placeholder file that applications can interact with
        if let Some(parent) = virtual_vol.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Create an info file for the volume
        let info_path = virtual_vol.file_path.with_extension("info");
        let mut info_file = fs::File::create(info_path)?;
        writeln!(info_file, "GalleonFS Volume Information")?;
        writeln!(info_file, "Volume ID: {}", virtual_vol.volume_id)?;
        writeln!(info_file, "Volume Name: {}", virtual_vol.volume.name)?;
        writeln!(info_file, "Volume Type: {:?}", virtual_vol.volume.volume_type)?;
        writeln!(info_file, "Volume State: {:?}", virtual_vol.volume.state)?;
        writeln!(info_file, "Size: {} bytes", virtual_vol.volume.size_bytes)?;
        writeln!(info_file, "Storage Class: {}", virtual_vol.volume.storage_class)?;
        writeln!(info_file, "Created: {:?}", virtual_vol.volume.created_at)?;
        
        Ok(())
    }
    
    pub async fn start_proxy_service(&self) -> Result<()> {
        loop {
            // Sync volumes periodically
            if let Err(e) = self.sync_volumes().await {
                tracing::error!("Failed to sync volumes: {}", e);
            }
            
            // Check for file operations and proxy them to GalleonFS
            self.process_file_operations().await?;
            
            // Wait before next check
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    
    async fn process_file_operations(&self) -> Result<()> {
        // This is a simplified implementation
        // In a real implementation, we would use file system watching
        // or implement a proper virtual file system driver
        
        let volumes_dir = self.mount_point.join("volumes");
        if !volumes_dir.exists() {
            return Ok(());
        }
        
        // List current .vol files and ensure they exist
        let volumes = self.volumes.read().await;
        for virtual_vol in volumes.values() {
            if !virtual_vol.file_path.exists() {
                // Create a placeholder file
                if let Ok(mut file) = fs::File::create(&virtual_vol.file_path) {
                    let _ = file.write_all(
                        format!("GalleonFS Volume: {} ({})\n", 
                               virtual_vol.volume.name, 
                               virtual_vol.volume_id).as_bytes()
                    );
                }
            }
        }
        
        Ok(())
    }
    
    pub fn get_mount_point(&self) -> &Path {
        &self.mount_point
    }
    
    pub async fn get_volume_by_path(&self, path: &Path) -> Option<VirtualVolume> {
        let volumes = self.volumes.read().await;
        
        for virtual_vol in volumes.values() {
            if virtual_vol.file_path == path {
                return Some(virtual_vol.clone());
            }
        }
        
        None
    }
}

impl Clone for VirtualVolume {
    fn clone(&self) -> Self {
        Self {
            volume_id: self.volume_id,
            volume: self.volume.clone(),
            file_path: self.file_path.clone(),
        }
    }
}

// Helper functions for applications to interact with GalleonFS volumes
pub struct GalleonVolumeFile {
    virtual_fs: Arc<GalleonVirtualFS>,
    volume: VirtualVolume,
    position: u64,
}

impl GalleonVolumeFile {
    pub async fn open(virtual_fs: Arc<GalleonVirtualFS>, volume_path: &Path) -> Result<Self> {
        let volume = virtual_fs.get_volume_by_path(volume_path).await
            .ok_or_else(|| anyhow::anyhow!("Volume not found: {}", volume_path.display()))?;
        
        Ok(Self {
            virtual_fs,
            volume,
            position: 0,
        })
    }
    
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let data = self.volume.read(
            &self.virtual_fs.galleonfs,
            self.position,
            buf.len() as u64
        ).await?;
        
        let len = std::cmp::min(buf.len(), data.len());
        buf[..len].copy_from_slice(&data[..len]);
        self.position += len as u64;
        
        Ok(len)
    }
    
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let written = self.volume.write(
            &self.virtual_fs.galleonfs,
            self.position,
            buf
        ).await?;
        
        self.position += written as u64;
        Ok(written)
    }
    
    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        match pos {
            SeekFrom::Start(offset) => {
                self.position = offset;
            }
            SeekFrom::Current(offset) => {
                if offset >= 0 {
                    self.position += offset as u64;
                } else {
                    self.position = self.position.saturating_sub((-offset) as u64);
                }
            }
            SeekFrom::End(offset) => {
                if offset >= 0 {
                    self.position = self.volume.volume.size_bytes + offset as u64;
                } else {
                    self.position = self.volume.volume.size_bytes.saturating_sub((-offset) as u64);
                }
            }
        }
        
        Ok(self.position)
    }
}