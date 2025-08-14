use crate::*;
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct SnapshotManager {
    storage_engine: Arc<dyn StorageEngine>,
    snapshot_policies: Arc<RwLock<HashMap<String, SnapshotPolicy>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotPolicy {
    pub name: String,
    pub schedule: String,
    pub retention_count: u32,
    pub volume_selector: LabelSelector,
}

impl SnapshotManager {
    pub fn new(storage_engine: Arc<dyn StorageEngine>) -> Self {
        Self {
            storage_engine,
            snapshot_policies: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_snapshot(&self, volume_id: Uuid, name: &str) -> Result<VolumeSnapshot> {
        self.storage_engine.create_snapshot(volume_id, name).await
    }

    pub async fn delete_snapshot(&self, snapshot_id: Uuid) -> Result<()> {
        self.storage_engine.delete_snapshot(snapshot_id).await
    }

    pub async fn list_snapshots(&self, volume_id: Uuid) -> Result<Vec<VolumeSnapshot>> {
        self.storage_engine.list_snapshots(volume_id).await
    }

    pub async fn restore_from_snapshot(&self, snapshot_id: Uuid, target_volume_id: Uuid) -> Result<()> {
        self.storage_engine.restore_snapshot(snapshot_id, target_volume_id).await
    }

    pub async fn create_volume_from_snapshot(&self, snapshot_id: Uuid, volume_name: &str) -> Result<Volume> {
        // Create a new volume and restore snapshot data to it
        let snapshots = self.storage_engine.list_snapshots(Uuid::nil()).await?;
        let snapshot = snapshots.iter()
            .find(|s| s.id == snapshot_id)
            .ok_or_else(|| anyhow::anyhow!("Snapshot not found"))?;

        let source_volume = self.storage_engine.get_volume(snapshot.volume_id).await?
            .ok_or_else(|| anyhow::anyhow!("Source volume not found"))?;

        let mut new_volume = Volume::new(
            volume_name.to_string(),
            source_volume.volume_type,
            snapshot.size_bytes,
            source_volume.storage_class.clone(),
        );

        self.storage_engine.create_volume(&mut new_volume).await?;
        self.storage_engine.restore_snapshot(snapshot_id, new_volume.id).await?;

        Ok(new_volume)
    }

    pub async fn add_snapshot_policy(&self, policy: SnapshotPolicy) -> Result<()> {
        let mut policies = self.snapshot_policies.write().await;
        policies.insert(policy.name.clone(), policy);
        Ok(())
    }

    pub async fn remove_snapshot_policy(&self, policy_name: &str) -> Result<()> {
        let mut policies = self.snapshot_policies.write().await;
        policies.remove(policy_name);
        Ok(())
    }

    pub async fn list_snapshot_policies(&self) -> Result<Vec<SnapshotPolicy>> {
        let policies = self.snapshot_policies.read().await;
        Ok(policies.values().cloned().collect())
    }
}