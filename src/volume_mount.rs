use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::fs;
use std::io::SeekFrom;
use tokio::sync::RwLock;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

use crate::{GalleonFS, Volume, WriteConcern, VolumeState, AccessMode, VolumeType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MountState {
    Mounting,
    Mounted,
    Unmounting, 
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    pub mount_id: Uuid,
    pub volume_id: Uuid,
    pub mount_point: PathBuf,
    pub mount_options: Vec<String>,
    pub state: MountState,
    pub mounted_at: Option<std::time::SystemTime>,
    pub mount_count: u32, // Reference counting for shared mounts
}

impl VolumeMount {
    pub fn new(volume_id: Uuid, mount_point: PathBuf, mount_options: Vec<String>) -> Self {
        Self {
            mount_id: Uuid::new_v4(),
            volume_id,
            mount_point,
            mount_options,
            state: MountState::Mounting,
            mounted_at: None,
            mount_count: 1,
        }
    }
}

pub struct VolumeMountManager {
    galleonfs: Arc<GalleonFS>,
    mounts: Arc<RwLock<HashMap<Uuid, VolumeMount>>>, // mount_id -> mount
    volume_mounts: Arc<RwLock<HashMap<Uuid, Vec<Uuid>>>>, // volume_id -> mount_ids
    path_mounts: Arc<RwLock<HashMap<PathBuf, Uuid>>>, // mount_point -> mount_id
}

impl VolumeMountManager {
    pub fn new(galleonfs: Arc<GalleonFS>) -> Self {
        Self {
            galleonfs,
            mounts: Arc::new(RwLock::new(HashMap::new())),
            volume_mounts: Arc::new(RwLock::new(HashMap::new())),
            path_mounts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Mount a volume at the specified path
    pub async fn mount_volume(
        &self,
        volume_id: Uuid,
        mount_point: PathBuf,
        mount_options: Vec<String>,
    ) -> Result<Uuid> {
        // Verify volume exists and is in correct state
        let volume = self.galleonfs.get_volume(volume_id).await?
            .ok_or_else(|| anyhow::anyhow!("Volume not found: {}", volume_id))?;

        if volume.state != VolumeState::Available {
            return Err(anyhow::anyhow!("Volume {} is not available for mounting (state: {:?})", volume_id, volume.state));
        }

        // Check if path is already mounted
        {
            let path_mounts = self.path_mounts.read().await;
            if path_mounts.contains_key(&mount_point) {
                return Err(anyhow::anyhow!("Path already mounted: {}", mount_point.display()));
            }
        }

        // Check if volume can support multiple mounts based on its characteristics
        let existing_mounts = self.get_volume_mounts(volume_id).await;
        if !existing_mounts.is_empty() {
            let can_multi_mount = self.can_support_multiple_mounts(&volume, &mount_options).await?;
            if !can_multi_mount {
                return Err(anyhow::anyhow!(
                    "Volume {} does not support multiple concurrent mounts (type: {:?}, access_modes: {:?})", 
                    volume_id, volume.volume_type, volume.access_modes
                ));
            }
        }

        // Create mount
        let mount = VolumeMount::new(volume_id, mount_point.clone(), mount_options);
        let mount_id = mount.mount_id;

        // Create mount point directory
        if let Some(parent) = mount_point.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&mount_point)?;

        // Create virtual volume file interface
        self.create_volume_interface(&mount_point, &volume).await?;

        // Update mount tracking
        {
            let mut mounts = self.mounts.write().await;
            let mut volume_mounts = self.volume_mounts.write().await;
            let mut path_mounts = self.path_mounts.write().await;

            // Update mount state
            let mut updated_mount = mount;
            updated_mount.state = MountState::Mounted;
            updated_mount.mounted_at = Some(std::time::SystemTime::now());

            mounts.insert(mount_id, updated_mount);
            path_mounts.insert(mount_point.clone(), mount_id);
            
            volume_mounts.entry(volume_id)
                .or_insert_with(Vec::new)
                .push(mount_id);
        }

        // Update volume state in GalleonFS (only if this is the first mount)
        let volume_mounts = self.volume_mounts.read().await;
        let mount_count = volume_mounts.get(&volume_id).map_or(0, |mounts| mounts.len());
        drop(volume_mounts);
        
        if mount_count == 1 {
            self.galleonfs.storage_engine.update_volume_state(volume_id, VolumeState::Mounted).await?;
        }

        tracing::info!("Volume {} mounted at {}", volume_id, mount_point.display());
        Ok(mount_id)
    }

    /// Unmount a volume by mount ID
    pub async fn unmount_volume(&self, mount_id: Uuid) -> Result<()> {
        let mount = {
            let mounts = self.mounts.read().await;
            mounts.get(&mount_id).cloned()
                .ok_or_else(|| anyhow::anyhow!("Mount not found: {}", mount_id))?
        };

        // Update mount state
        {
            let mut mounts = self.mounts.write().await;
            if let Some(mount_ref) = mounts.get_mut(&mount_id) {
                mount_ref.state = MountState::Unmounting;
            }
        }

        // Remove volume interface
        self.cleanup_volume_interface(&mount.mount_point).await?;

        // Update tracking
        {
            let mut mounts = self.mounts.write().await;
            let mut volume_mounts = self.volume_mounts.write().await;
            let mut path_mounts = self.path_mounts.write().await;

            mounts.remove(&mount_id);
            path_mounts.remove(&mount.mount_point);
            
            if let Some(mount_list) = volume_mounts.get_mut(&mount.volume_id) {
                mount_list.retain(|&id| id != mount_id);
                if mount_list.is_empty() {
                    volume_mounts.remove(&mount.volume_id);
                    // Update volume state if no more mounts
                    if let Err(e) = self.galleonfs.storage_engine
                        .update_volume_state(mount.volume_id, VolumeState::Available).await {
                        tracing::warn!("Failed to update volume state after unmount: {}", e);
                    }
                }
            }
        }

        // Remove mount point directory if empty
        if mount.mount_point.exists() {
            if let Ok(entries) = fs::read_dir(&mount.mount_point) {
                if entries.count() == 0 {
                    let _ = fs::remove_dir(&mount.mount_point);
                }
            }
        }

        tracing::info!("Volume {} unmounted from {}", mount.volume_id, mount.mount_point.display());
        Ok(())
    }

    /// Unmount a volume by path
    pub async fn unmount_path(&self, mount_point: &Path) -> Result<()> {
        let mount_id = {
            let path_mounts = self.path_mounts.read().await;
            *path_mounts.get(mount_point)
                .ok_or_else(|| anyhow::anyhow!("No mount at path: {}", mount_point.display()))?
        };

        self.unmount_volume(mount_id).await
    }

    /// Get all mounts for a volume
    pub async fn get_volume_mounts(&self, volume_id: Uuid) -> Vec<VolumeMount> {
        let volume_mounts = self.volume_mounts.read().await;
        let mounts = self.mounts.read().await;

        if let Some(mount_ids) = volume_mounts.get(&volume_id) {
            mount_ids.iter()
                .filter_map(|&mount_id| mounts.get(&mount_id).cloned())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// List all active mounts
    pub async fn list_mounts(&self) -> Vec<VolumeMount> {
        let mounts = self.mounts.read().await;
        mounts.values().cloned().collect()
    }

    /// Get mount by path
    pub async fn get_mount_by_path(&self, mount_point: &Path) -> Option<VolumeMount> {
        let path_mounts = self.path_mounts.read().await;
        let mounts = self.mounts.read().await;

        if let Some(&mount_id) = path_mounts.get(mount_point) {
            mounts.get(&mount_id).cloned()
        } else {
            None
        }
    }

    /// Check if a volume can support multiple concurrent mounts
    async fn can_support_multiple_mounts(&self, volume: &Volume, mount_options: &[String]) -> Result<bool> {
        // Check if this is a read-only mount
        let is_readonly = mount_options.iter().any(|opt| opt == "ro" || opt == "readonly");
        
        // ReadOnlyMany volumes can always be mounted multiple times
        if volume.access_modes.contains(&AccessMode::ReadOnlyMany) {
            return Ok(true);
        }
        
        // ReadWriteMany volumes support multiple concurrent mounts
        if volume.access_modes.contains(&AccessMode::ReadWriteMany) {
            return Ok(true);
        }
        
        // Shared volumes can be mounted multiple times with appropriate access controls
        if volume.volume_type == VolumeType::Shared {
            return Ok(true);
        }
        
        // Allow multiple read-only mounts of any volume (useful for monitoring, backups, etc.)
        if is_readonly {
            return Ok(true);
        }
        
        // Check if volume has replication enabled (replicated volumes are safer for multiple mounts)
        let is_replicated = self.is_volume_replicated(volume).await?;
        if is_replicated {
            tracing::info!("Allowing multiple mounts for replicated volume {}", volume.id);
            return Ok(true);
        }
        
        // ReadWriteOnce volumes with persistent type generally don't support multiple mounts
        // unless they're specifically designed for it
        if volume.access_modes.contains(&AccessMode::ReadWriteOnce) && 
           volume.volume_type == VolumeType::Persistent {
            return Ok(false);
        }
        
        // Default to allowing multiple mounts (can be made more restrictive as needed)
        Ok(true)
    }
    
    /// Check if a volume is replicated
    async fn is_volume_replicated(&self, volume: &Volume) -> Result<bool> {
        // Check storage class parameters for replication
        if let Some(storage_class) = self.galleonfs.get_storage_class(&volume.storage_class).await? {
            if let Some(replication) = storage_class.parameters.get("replication") {
                if let Ok(replica_count) = replication.parse::<i32>() {
                    return Ok(replica_count > 1);
                }
            }
            
            // Check for distributed storage classes
            if storage_class.provisioner.contains("distributed") {
                return Ok(true);
            }
        }
        
        // Check if volume is in a replicated storage pool
        // This would require additional metadata tracking, but for now we'll use heuristics
        
        // Assume encrypted storage is replicated by default (as per our storage class setup)
        if volume.storage_class.contains("encrypted") || 
           volume.storage_class.contains("distributed") ||
           volume.storage_class.contains("replicated") {
            return Ok(true);
        }
        
        Ok(false)
    }

    /// Create volume interface at mount point
    async fn create_volume_interface(&self, mount_point: &Path, volume: &Volume) -> Result<()> {
        // Create volume data file
        let volume_file = mount_point.join("data");
        fs::File::create(&volume_file)?;

        // Create volume metadata file  
        let metadata_file = mount_point.join(".galleonfs_metadata");
        let metadata = serde_json::json!({
            "volume_id": volume.id,
            "volume_name": volume.name,
            "volume_type": volume.volume_type,
            "size_bytes": volume.size_bytes,
            "storage_class": volume.storage_class,
            "created_at": volume.created_at,
        });
        fs::write(metadata_file, serde_json::to_string_pretty(&metadata)?)?;

        // Create README for users
        let readme_file = mount_point.join("README.txt");
        let readme_content = format!(
            "GalleonFS Volume Mount\n\
             =====================\n\
             \n\
             Volume: {} ({})\n\
             Type: {:?}\n\
             Size: {} bytes\n\
             Storage Class: {}\n\
             \n\
             Usage:\n\
             - Read data: cat data\n\
             - Write data: echo 'content' > data\n\
             - Copy files: cp myfile.txt data\n\
             \n\
             The 'data' file represents the volume content.\n\
             All operations on this file are backed by GalleonFS storage.\n",
            volume.name,
            volume.id,
            volume.volume_type,
            volume.size_bytes,
            volume.storage_class
        );
        fs::write(readme_file, readme_content)?;

        Ok(())
    }

    /// Clean up volume interface
    async fn cleanup_volume_interface(&self, mount_point: &Path) -> Result<()> {
        if mount_point.exists() {
            fs::remove_dir_all(mount_point)?;
        }
        Ok(())
    }

    /// Create a volume file handle for applications
    pub async fn open_volume_file(&self, mount_point: &Path) -> Result<GalleonVolumeFile> {
        let mount = self.get_mount_by_path(mount_point).await
            .ok_or_else(|| anyhow::anyhow!("No volume mounted at: {}", mount_point.display()))?;

        let volume = self.galleonfs.get_volume(mount.volume_id).await?
            .ok_or_else(|| anyhow::anyhow!("Volume not found: {}", mount.volume_id))?;

        Ok(GalleonVolumeFile::new(
            self.galleonfs.clone(),
            mount.volume_id,
            volume.size_bytes,
        ))
    }
}

/// File handle for reading/writing volume data
pub struct GalleonVolumeFile {
    galleonfs: Arc<GalleonFS>,
    volume_id: Uuid,
    volume_size: u64,
    position: u64,
}

impl GalleonVolumeFile {
    fn new(galleonfs: Arc<GalleonFS>, volume_id: Uuid, volume_size: u64) -> Self {
        Self {
            galleonfs,
            volume_id,
            volume_size,
            position: 0,
        }
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let block_size = 4096u64;
        let start_block = self.position / block_size;
        let end_block = (self.position + buf.len() as u64 + block_size - 1) / block_size;
        
        let mut data = Vec::new();
        
        for block_id in start_block..end_block {
            match self.galleonfs.read_block(self.volume_id, block_id).await {
                Ok(block_data) => {
                    let block_start = if block_id == start_block {
                        (self.position % block_size) as usize
                    } else {
                        0
                    };
                    
                    let remaining_in_buf = buf.len() - data.len();
                    let block_end = std::cmp::min(
                        block_data.len(),
                        block_start + remaining_in_buf
                    );
                    
                    if block_start < block_data.len() {
                        data.extend_from_slice(&block_data[block_start..block_end]);
                    }
                }
                Err(_) => {
                    // Block doesn't exist, pad with zeros
                    let zeros_needed = std::cmp::min(
                        block_size as usize,
                        buf.len() - data.len()
                    );
                    data.extend(std::iter::repeat(0).take(zeros_needed));
                }
            }
            
            if data.len() >= buf.len() {
                break;
            }
        }
        
        let len = std::cmp::min(buf.len(), data.len());
        buf[..len].copy_from_slice(&data[..len]);
        self.position += len as u64;
        
        Ok(len)
    }

    pub async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let block_size = 4096u64;
        let start_block = self.position / block_size;
        let mut written = 0;

        for (i, chunk) in buf.chunks(block_size as usize).enumerate() {
            let block_id = start_block + i as u64;
            let block_offset = if i == 0 { self.position % block_size } else { 0 };

            if block_offset == 0 && chunk.len() == block_size as usize {
                // Full block write
                self.galleonfs.write_block(
                    self.volume_id,
                    block_id,
                    chunk,
                    WriteConcern::WriteDurable
                ).await?;
                written += chunk.len();
            } else {
                // Partial block write - read-modify-write
                let mut block_data = match self.galleonfs.read_block(self.volume_id, block_id).await {
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
                    self.volume_id,
                    block_id,
                    &block_data,
                    WriteConcern::WriteDurable
                ).await?;
                
                written += copy_len;
            }
        }

        self.position += written as u64;
        Ok(written)
    }

    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        match pos {
            SeekFrom::Start(offset) => {
                self.position = std::cmp::min(offset, self.volume_size);
            }
            SeekFrom::Current(offset) => {
                if offset >= 0 {
                    self.position = std::cmp::min(
                        self.position + offset as u64,
                        self.volume_size
                    );
                } else {
                    self.position = self.position.saturating_sub((-offset) as u64);
                }
            }
            SeekFrom::End(offset) => {
                if offset >= 0 {
                    self.position = std::cmp::min(
                        self.volume_size + offset as u64,
                        self.volume_size
                    );
                } else {
                    self.position = self.volume_size.saturating_sub((-offset) as u64);
                }
            }
        }
        
        Ok(self.position)
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    pub fn volume_size(&self) -> u64 {
        self.volume_size
    }

    pub fn volume_id(&self) -> Uuid {
        self.volume_id
    }
}

impl Clone for VolumeMountManager {
    fn clone(&self) -> Self {
        Self {
            galleonfs: self.galleonfs.clone(),
            mounts: self.mounts.clone(),
            volume_mounts: self.volume_mounts.clone(),
            path_mounts: self.path_mounts.clone(),
        }
    }
}