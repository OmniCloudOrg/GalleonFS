use galleon_common::{config::GalleonConfig, error::Result, types::*};
use std::sync::Arc;

pub struct TopologyManager {
    config: GalleonConfig,
}

impl TopologyManager {
    pub async fn new(config: &GalleonConfig, _metadata_store: Arc<super::MetadataStore>) -> Result<Self> {
        Ok(Self { config: config.clone() })
    }
    pub async fn start(&self) -> Result<()> { Ok(()) }
    pub async fn stop(&self) -> Result<()> { Ok(()) }
    pub async fn register_device(&self, _device: Device) -> Result<()> { Ok(()) }
    pub async fn get_all_devices(&self) -> Result<Vec<Device>> { Ok(Vec::new()) }
}
