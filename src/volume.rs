use crate::*;
use anyhow::Result;

pub struct VolumeManager {
    storage_engine: Arc<dyn StorageEngine>,
}

impl VolumeManager {
    pub fn new(storage_engine: Arc<dyn StorageEngine>) -> Self {
        Self { storage_engine }
    }

    pub async fn provision_volume(&self, claim: &PersistentVolumeClaim) -> Result<Volume> {
        let default_class = "default".to_string();
        let storage_class = claim.spec.storage_class.as_ref()
            .unwrap_or(&default_class);
        
        let size_str = claim.spec.resources.requests.get("storage")
            .ok_or_else(|| anyhow::anyhow!("No storage size specified in claim"))?;
        
        let size_bytes = self.parse_storage_size(size_str)?;
        
        let mut volume = Volume::new(
            claim.name.clone(),
            VolumeType::Persistent,
            size_bytes,
            storage_class.clone(),
        );
        
        volume.access_modes = claim.spec.access_modes.clone();
        volume.claim_ref = Some(claim.id.to_string());
        
        self.storage_engine.create_volume(&mut volume).await?;
        
        Ok(volume)
    }
    
    pub async fn bind_volume(&self, volume_id: Uuid, _claim_id: Uuid) -> Result<()> {
        self.storage_engine.update_volume_state(volume_id, VolumeState::Bound).await?;
        Ok(())
    }
    
    pub async fn release_volume(&self, volume_id: Uuid) -> Result<()> {
        self.storage_engine.update_volume_state(volume_id, VolumeState::Released).await?;
        Ok(())
    }
    
    fn parse_storage_size(&self, size_str: &str) -> Result<u64> {
        let size_str = size_str.trim();
        if size_str.ends_with("Gi") {
            let num = size_str.trim_end_matches("Gi").parse::<u64>()?;
            Ok(num * 1024 * 1024 * 1024)
        } else if size_str.ends_with("Mi") {
            let num = size_str.trim_end_matches("Mi").parse::<u64>()?;
            Ok(num * 1024 * 1024)
        } else if size_str.ends_with("Ki") {
            let num = size_str.trim_end_matches("Ki").parse::<u64>()?;
            Ok(num * 1024)
        } else {
            size_str.parse::<u64>().map_err(|e| anyhow::anyhow!("Invalid size format: {}", e))
        }
    }
}