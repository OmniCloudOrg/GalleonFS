use crate::*;
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct BackupManager {
    storage_engine: Arc<dyn StorageEngine>,
    backups: Arc<RwLock<HashMap<Uuid, Backup>>>,
    backup_repository: BackupRepository,
}

pub struct BackupRepository {
    storage_path: PathBuf,
}

impl BackupRepository {
    pub fn new(storage_path: PathBuf) -> Self {
        std::fs::create_dir_all(&storage_path).expect("Failed to create backup directory");
        Self { storage_path }
    }

    pub async fn store_backup(&self, backup: &Backup, data: &[u8]) -> Result<()> {
        let backup_path = self.storage_path.join(format!("{}.backup", backup.id));
        std::fs::write(backup_path, data)?;
        Ok(())
    }

    pub async fn retrieve_backup(&self, backup_id: Uuid) -> Result<Vec<u8>> {
        let backup_path = self.storage_path.join(format!("{}.backup", backup_id));
        let data = std::fs::read(backup_path)?;
        Ok(data)
    }

    pub async fn delete_backup(&self, backup_id: Uuid) -> Result<()> {
        let backup_path = self.storage_path.join(format!("{}.backup", backup_id));
        if backup_path.exists() {
            std::fs::remove_file(backup_path)?;
        }
        Ok(())
    }
}

impl BackupManager {
    pub fn new(storage_engine: Arc<dyn StorageEngine>, repository_path: PathBuf) -> Self {
        Self {
            storage_engine,
            backups: Arc::new(RwLock::new(HashMap::new())),
            backup_repository: BackupRepository::new(repository_path),
        }
    }

    pub async fn create_backup(&self, volume_id: Uuid, policy_name: &str, strategy: BackupStrategy) -> Result<Backup> {
        let volume = self.storage_engine.get_volume(volume_id).await?
            .ok_or_else(|| anyhow::anyhow!("Volume not found"))?;

        let backup = Backup {
            id: Uuid::new_v4(),
            name: format!("backup-{}-{}", volume.name, chrono::Utc::now().format("%Y%m%d%H%M%S")),
            volume_id,
            policy_name: policy_name.to_string(),
            strategy,
            size_bytes: volume.size_bytes,
            state: BackupState::InProgress,
            created_at: SystemTime::now(),
            completed_at: None,
        };

        // Create snapshot first for consistency
        let snapshot = self.storage_engine.create_snapshot(volume_id, &format!("backup-{}", backup.id)).await?;

        // For simplicity, we'll backup the entire volume
        // In a real implementation, this would be optimized based on strategy
        let backup_data = self.read_volume_data(volume_id, &volume).await?;
        self.backup_repository.store_backup(&backup, &backup_data).await?;

        let mut completed_backup = backup.clone();
        completed_backup.state = BackupState::Completed;
        completed_backup.completed_at = Some(SystemTime::now());

        let mut backups = self.backups.write().await;
        backups.insert(completed_backup.id, completed_backup.clone());

        // Clean up snapshot used for backup
        self.storage_engine.delete_snapshot(snapshot.id).await?;

        Ok(completed_backup)
    }

    pub async fn restore_backup(&self, backup_id: Uuid, target_volume_id: Uuid) -> Result<()> {
        let backups = self.backups.read().await;
        let backup = backups.get(&backup_id)
            .ok_or_else(|| anyhow::anyhow!("Backup not found"))?;

        if backup.state != BackupState::Completed {
            return Err(anyhow::anyhow!("Backup is not in completed state"));
        }

        let backup_data = self.backup_repository.retrieve_backup(backup_id).await?;
        self.restore_volume_data(target_volume_id, &backup_data).await?;

        Ok(())
    }

    pub async fn list_backups(&self, volume_id: Option<Uuid>) -> Result<Vec<Backup>> {
        let backups = self.backups.read().await;
        if let Some(vol_id) = volume_id {
            Ok(backups.values()
                .filter(|b| b.volume_id == vol_id)
                .cloned()
                .collect())
        } else {
            Ok(backups.values().cloned().collect())
        }
    }

    pub async fn delete_backup(&self, backup_id: Uuid) -> Result<()> {
        self.backup_repository.delete_backup(backup_id).await?;
        
        let mut backups = self.backups.write().await;
        backups.remove(&backup_id);

        Ok(())
    }

    async fn read_volume_data(&self, volume_id: Uuid, volume: &Volume) -> Result<Vec<u8>> {
        let block_size = 4096u64; // Default block size
        let num_blocks = (volume.size_bytes + block_size - 1) / block_size;
        let mut data = Vec::new();

        for block_id in 0..num_blocks {
            match self.storage_engine.read_block(volume_id, block_id).await {
                Ok(block_data) => data.extend_from_slice(&block_data),
                Err(_) => {
                    // Handle sparse volumes by writing zeros for unallocated blocks
                    data.extend_from_slice(&vec![0u8; block_size as usize]);
                }
            }
        }

        Ok(data)
    }

    async fn restore_volume_data(&self, volume_id: Uuid, data: &[u8]) -> Result<()> {
        let block_size = 4096u64; // Default block size
        let num_blocks = (data.len() as u64 + block_size - 1) / block_size;

        for block_id in 0..num_blocks {
            let start_offset = (block_id * block_size) as usize;
            let end_offset = std::cmp::min(start_offset + block_size as usize, data.len());
            
            if start_offset < data.len() {
                let block_data = &data[start_offset..end_offset];
                self.storage_engine.write_block(volume_id, block_id, block_data).await?;
            }
        }

        self.storage_engine.flush_volume(volume_id).await?;
        Ok(())
    }
}