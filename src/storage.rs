use crate::*;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

pub struct FileStorageEngine {
    volumes: Arc<RwLock<HashMap<Uuid, Volume>>>,
    snapshots: Arc<RwLock<HashMap<Uuid, VolumeSnapshot>>>,
    storage_path: PathBuf,
    block_size: u64,
    metrics: Arc<RwLock<HashMap<Uuid, VolumeMetrics>>>,
    journal_entries: Arc<RwLock<HashMap<Uuid, JournalEntry>>>,
}

impl FileStorageEngine {
    pub fn new(storage_path: PathBuf, block_size: u64) -> Self {
        std::fs::create_dir_all(&storage_path).expect("Failed to create storage directory");
        Self {
            volumes: Arc::new(RwLock::new(HashMap::new())),
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            storage_path,
            block_size,
            metrics: Arc::new(RwLock::new(HashMap::new())),
            journal_entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn get_volume_path(&self, volume_id: Uuid) -> PathBuf {
        self.storage_path.join(format!("{}.vol", volume_id))
    }

    fn get_snapshot_path(&self, snapshot_id: Uuid) -> PathBuf {
        self.storage_path.join("snapshots").join(format!("{}.snap", snapshot_id))
    }

    fn get_journal_path(&self) -> PathBuf {
        self.storage_path.join("journal.log")
    }

    fn get_metrics_path(&self, volume_id: Uuid) -> PathBuf {
        self.storage_path.join("metrics").join(format!("{}.metrics", volume_id))
    }

    async fn update_metrics(&self, volume_id: Uuid, iops: f64, throughput: f64, latency: f64) -> Result<()> {
        let metrics = VolumeMetrics {
            volume_id,
            timestamp: SystemTime::now(),
            iops,
            throughput_mbps: throughput,
            latency_ms: latency,
            queue_depth: 1.0,
            cache_hit_rate: 0.8,
            error_count: 0,
        };

        let mut metrics_map = self.metrics.write().await;
        metrics_map.insert(volume_id, metrics.clone());

        // Persist metrics to disk
        let metrics_path = self.get_metrics_path(volume_id);
        std::fs::create_dir_all(metrics_path.parent().unwrap())?;
        let metrics_data = bincode::serialize(&metrics)?;
        std::fs::write(metrics_path, metrics_data)?;

        Ok(())
    }
}

#[async_trait]
impl StorageEngine for FileStorageEngine {
    async fn create_volume(&self, volume: &mut Volume) -> Result<()> {
        let volume_path = self.get_volume_path(volume.id);
        
        let mut file = File::create(&volume_path)?;
        file.set_len(volume.size_bytes)?;
        file.flush()?;

        volume.state = VolumeState::Available;
        volume.updated_at = SystemTime::now();
        
        let mut volumes = self.volumes.write().await;
        volumes.insert(volume.id, volume.clone());
        
        Ok(())
    }

    async fn delete_volume(&self, volume_id: Uuid) -> Result<()> {
        let volume_path = self.get_volume_path(volume_id);
        if volume_path.exists() {
            std::fs::remove_file(volume_path)?;
        }

        let mut volumes = self.volumes.write().await;
        volumes.remove(&volume_id);

        // Clean up metrics
        let mut metrics = self.metrics.write().await;
        metrics.remove(&volume_id);

        Ok(())
    }

    async fn get_volume(&self, volume_id: Uuid) -> Result<Option<Volume>> {
        let volumes = self.volumes.read().await;
        Ok(volumes.get(&volume_id).cloned())
    }

    async fn list_volumes(&self) -> Result<Vec<Volume>> {
        let volumes = self.volumes.read().await;
        Ok(volumes.values().cloned().collect())
    }

    async fn update_volume_state(&self, volume_id: Uuid, state: VolumeState) -> Result<()> {
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(&volume_id) {
            volume.state = state;
            volume.updated_at = SystemTime::now();
        }
        Ok(())
    }

    async fn expand_volume(&self, volume_id: Uuid, new_size: u64) -> Result<()> {
        let volume_path = self.get_volume_path(volume_id);
        let mut file = OpenOptions::new().write(true).open(&volume_path)?;
        file.set_len(new_size)?;
        file.flush()?;

        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(&volume_id) {
            volume.size_bytes = new_size;
            volume.updated_at = SystemTime::now();
        }

        Ok(())
    }

    async fn shrink_volume(&self, volume_id: Uuid, new_size: u64) -> Result<()> {
        let current_volume = self.get_volume(volume_id).await?;
        if let Some(volume) = current_volume {
            if new_size >= volume.size_bytes {
                return Err(anyhow!("New size must be smaller than current size"));
            }
            // Note: Shrinking is filesystem dependent and may not be supported
            self.expand_volume(volume_id, new_size).await
        } else {
            Err(anyhow!("Volume not found"))
        }
    }

    async fn attach_volume(&self, volume_id: Uuid, node: &str) -> Result<()> {
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(&volume_id) {
            if !volume.attached_nodes.contains(&node.to_string()) {
                volume.attached_nodes.push(node.to_string());
                volume.state = VolumeState::Bound;
                volume.updated_at = SystemTime::now();
            }
        }
        Ok(())
    }

    async fn detach_volume(&self, volume_id: Uuid, node: &str) -> Result<()> {
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(&volume_id) {
            volume.attached_nodes.retain(|n| n != node);
            if volume.attached_nodes.is_empty() {
                volume.state = VolumeState::Available;
            }
            volume.updated_at = SystemTime::now();
        }
        Ok(())
    }

    async fn mount_volume(&self, volume_id: Uuid, mount_path: &str) -> Result<()> {
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(&volume_id) {
            volume.mount_path = Some(PathBuf::from(mount_path));
            volume.state = VolumeState::Mounted;
            volume.updated_at = SystemTime::now();
        }
        Ok(())
    }

    async fn unmount_volume(&self, volume_id: Uuid) -> Result<()> {
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(&volume_id) {
            volume.mount_path = None;
            volume.state = if volume.attached_nodes.is_empty() {
                VolumeState::Available
            } else {
                VolumeState::Bound
            };
            volume.updated_at = SystemTime::now();
        }
        Ok(())
    }

    async fn write_block(&self, volume_id: Uuid, block_id: u64, data: &[u8]) -> Result<()> {
        let start_time = SystemTime::now();
        
        let volume_path = self.get_volume_path(volume_id);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&volume_path)?;

        let offset = block_id * self.block_size;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;

        let latency = start_time.elapsed().unwrap_or_default().as_millis() as f64;
        let throughput = (data.len() as f64) / (1024.0 * 1024.0); // MB
        
        self.update_metrics(volume_id, 1.0, throughput, latency).await?;

        Ok(())
    }

    async fn read_block(&self, volume_id: Uuid, block_id: u64) -> Result<Vec<u8>> {
        let start_time = SystemTime::now();
        
        let volume_path = self.get_volume_path(volume_id);
        let mut file = File::open(&volume_path)?;

        let offset = block_id * self.block_size;
        file.seek(SeekFrom::Start(offset))?;
        
        let mut buffer = vec![0u8; self.block_size as usize];
        file.read_exact(&mut buffer)?;

        let latency = start_time.elapsed().unwrap_or_default().as_millis() as f64;
        let throughput = (buffer.len() as f64) / (1024.0 * 1024.0); // MB
        
        self.update_metrics(volume_id, 1.0, throughput, latency).await?;

        Ok(buffer)
    }

    async fn flush_volume(&self, volume_id: Uuid) -> Result<()> {
        let volume_path = self.get_volume_path(volume_id);
        let mut file = OpenOptions::new()
            .write(true)
            .open(&volume_path)?;
        file.flush()?;
        Ok(())
    }

    async fn sync_volume(&self, volume_id: Uuid) -> Result<()> {
        let volume_path = self.get_volume_path(volume_id);
        let file = OpenOptions::new()
            .write(true)
            .open(&volume_path)?;
        file.sync_all()?;
        Ok(())
    }

    async fn create_snapshot(&self, volume_id: Uuid, snapshot_name: &str) -> Result<VolumeSnapshot> {
        let volume = self.get_volume(volume_id).await?
            .ok_or_else(|| anyhow!("Volume not found"))?;

        let snapshot = VolumeSnapshot {
            id: Uuid::new_v4(),
            name: snapshot_name.to_string(),
            volume_id,
            size_bytes: volume.size_bytes,
            state: SnapshotState::Pending,
            created_at: SystemTime::now(),
            ready_to_use: false,
            source_volume_mode: "Filesystem".to_string(),
            labels: HashMap::new(),
        };

        // Create snapshot directory
        let snapshot_dir = self.storage_path.join("snapshots");
        std::fs::create_dir_all(&snapshot_dir)?;

        // Copy volume data to snapshot
        let volume_path = self.get_volume_path(volume_id);
        let snapshot_path = self.get_snapshot_path(snapshot.id);
        std::fs::copy(volume_path, snapshot_path)?;

        let mut completed_snapshot = snapshot.clone();
        completed_snapshot.state = SnapshotState::Ready;
        completed_snapshot.ready_to_use = true;

        let mut snapshots = self.snapshots.write().await;
        snapshots.insert(completed_snapshot.id, completed_snapshot.clone());

        Ok(completed_snapshot)
    }

    async fn delete_snapshot(&self, snapshot_id: Uuid) -> Result<()> {
        let snapshot_path = self.get_snapshot_path(snapshot_id);
        if snapshot_path.exists() {
            std::fs::remove_file(snapshot_path)?;
        }

        let mut snapshots = self.snapshots.write().await;
        snapshots.remove(&snapshot_id);

        Ok(())
    }

    async fn list_snapshots(&self, volume_id: Uuid) -> Result<Vec<VolumeSnapshot>> {
        let snapshots = self.snapshots.read().await;
        Ok(snapshots.values()
            .filter(|s| s.volume_id == volume_id)
            .cloned()
            .collect())
    }

    async fn restore_snapshot(&self, snapshot_id: Uuid, target_volume_id: Uuid) -> Result<()> {
        let snapshot_path = self.get_snapshot_path(snapshot_id);
        let target_path = self.get_volume_path(target_volume_id);

        if !snapshot_path.exists() {
            return Err(anyhow!("Snapshot not found"));
        }

        std::fs::copy(snapshot_path, target_path)?;

        // Update volume state
        self.update_volume_state(target_volume_id, VolumeState::Available).await?;

        Ok(())
    }

    async fn clone_volume(&self, source_id: Uuid, target_name: &str) -> Result<Volume> {
        let source_volume = self.get_volume(source_id).await?
            .ok_or_else(|| anyhow!("Source volume not found"))?;

        let mut target_volume = Volume::new(
            target_name.to_string(),
            source_volume.volume_type,
            source_volume.size_bytes,
            source_volume.storage_class.clone(),
        );

        // Copy the volume file
        let source_path = self.get_volume_path(source_id);
        let target_path = self.get_volume_path(target_volume.id);
        std::fs::copy(source_path, target_path)?;

        target_volume.state = VolumeState::Available;
        target_volume.access_modes = source_volume.access_modes.clone();
        target_volume.labels = source_volume.labels.clone();

        let mut volumes = self.volumes.write().await;
        volumes.insert(target_volume.id, target_volume.clone());

        Ok(target_volume)
    }

    async fn transform_volume(&self, volume_id: Uuid, target_class: &str) -> Result<()> {
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(&volume_id) {
            volume.storage_class = target_class.to_string();
            volume.updated_at = SystemTime::now();
        }
        Ok(())
    }

    async fn write_journal_entry(&self, entry: &JournalEntry) -> Result<()> {
        let journal_path = self.get_journal_path();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&journal_path)?;
        
        let serialized = bincode::serialize(entry)?;
        file.write_all(&serialized)?;
        file.write_all(b"\n")?;
        file.flush()?;

        let mut entries = self.journal_entries.write().await;
        entries.insert(entry.id, entry.clone());
        
        Ok(())
    }

    async fn commit_journal_entry(&self, entry_id: Uuid) -> Result<()> {
        let mut entries = self.journal_entries.write().await;
        if let Some(entry) = entries.get_mut(&entry_id) {
            entry.committed = true;

            // Write commit entry to journal
            let journal_path = self.get_journal_path();
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&journal_path)?;
            
            let commit_entry = JournalEntry {
                id: Uuid::new_v4(),
                volume_id: entry.volume_id,
                operation: format!("COMMIT:{}", entry_id),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                committed: true,
                data: Vec::new(),
            };
            
            let serialized = bincode::serialize(&commit_entry)?;
            file.write_all(&serialized)?;
            file.write_all(b"\n")?;
            file.flush()?;
        }
        
        Ok(())
    }

    async fn recover_from_journal(&self, volume_id: Uuid) -> Result<()> {
        let journal_path = self.get_journal_path();
        if !journal_path.exists() {
            return Ok(());
        }

        let journal_data = std::fs::read_to_string(journal_path)?;
        let mut uncommitted_entries = Vec::new();

        for line in journal_data.lines() {
            if let Ok(entry) = bincode::deserialize::<JournalEntry>(line.as_bytes()) {
                if entry.volume_id == volume_id && !entry.committed {
                    uncommitted_entries.push(entry);
                }
            }
        }

        // Replay uncommitted entries or roll them back
        for entry in uncommitted_entries {
            if entry.operation.starts_with("WRITE:") {
                // Re-apply the write operation
                let parts: Vec<&str> = entry.operation.split(':').collect();
                if parts.len() >= 3 {
                    if let Ok(block_id) = parts[2].parse::<u64>() {
                        self.write_block(volume_id, block_id, &entry.data).await?;
                        self.commit_journal_entry(entry.id).await?;
                    }
                }
            }
        }

        Ok(())
    }

    async fn get_volume_metrics(&self, volume_id: Uuid) -> Result<VolumeMetrics> {
        let metrics = self.metrics.read().await;
        if let Some(metrics) = metrics.get(&volume_id) {
            Ok(metrics.clone())
        } else {
            // Return default metrics if none exist
            Ok(VolumeMetrics {
                volume_id,
                timestamp: SystemTime::now(),
                iops: 0.0,
                throughput_mbps: 0.0,
                latency_ms: 0.0,
                queue_depth: 0.0,
                cache_hit_rate: 0.0,
                error_count: 0,
            })
        }
    }

    async fn get_volume_usage(&self, volume_id: Uuid) -> Result<u64> {
        let volume_path = self.get_volume_path(volume_id);
        if let Ok(metadata) = std::fs::metadata(volume_path) {
            Ok(metadata.len())
        } else {
            Ok(0)
        }
    }

    async fn defragment_volume(&self, _volume_id: Uuid) -> Result<()> {
        // File-based storage doesn't need defragmentation
        // This would be implemented for more sophisticated storage backends
        tracing::info!("Defragmentation not needed for file-based storage");
        Ok(())
    }

    async fn encrypt_volume(&self, volume_id: Uuid, _config: &EncryptionConfig) -> Result<()> {
        // Mark volume as encrypted in metadata
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(&volume_id) {
            volume.labels.insert("encrypted".to_string(), "true".to_string());
            volume.updated_at = SystemTime::now();
        }
        Ok(())
    }

    async fn decrypt_volume(&self, volume_id: Uuid) -> Result<()> {
        // Remove encryption marker from metadata
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(&volume_id) {
            volume.labels.remove("encrypted");
            volume.updated_at = SystemTime::now();
        }
        Ok(())
    }

    async fn rotate_encryption_key(&self, volume_id: Uuid) -> Result<()> {
        // Update key rotation timestamp in metadata
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(&volume_id) {
            volume.labels.insert(
                "key_rotation".to_string(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string()
            );
            volume.updated_at = SystemTime::now();
        }
        Ok(())
    }
}