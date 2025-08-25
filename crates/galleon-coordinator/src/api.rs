use galleon_common::{config::GalleonConfig, error::Result};
use std::sync::Arc;

pub struct CoordinatorApi {
    config: GalleonConfig,
}

impl CoordinatorApi {
    pub async fn new(
        config: &GalleonConfig,
        _cluster_manager: Arc<super::ClusterManager>,
        _volume_manager: Arc<super::VolumeManager>,
        _topology_manager: Arc<super::TopologyManager>,
        _migration_manager: Arc<super::MigrationManager>,
    ) -> Result<Self> {
        Ok(Self { config: config.clone() })
    }
    pub async fn start(&self) -> Result<()> { Ok(()) }
    pub async fn stop(&self) -> Result<()> { Ok(()) }
}
