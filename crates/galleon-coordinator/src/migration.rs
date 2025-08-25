use galleon_common::{config::GalleonConfig, error::Result, types::*};
use std::sync::Arc;

pub struct MigrationManager {
    config: GalleonConfig,
}

impl MigrationManager {
    pub async fn new(
        config: &GalleonConfig,
        _metadata_store: Arc<super::MetadataStore>,
        _cluster_manager: Arc<super::ClusterManager>,
    ) -> Result<Self> {
        Ok(Self { config: config.clone() })
    }
    pub async fn start(&self) -> Result<()> { Ok(()) }
    pub async fn stop(&self) -> Result<()> { Ok(()) }
    pub async fn handle_node_removal(&self, _node_id: NodeId) -> Result<()> { Ok(()) }
}
