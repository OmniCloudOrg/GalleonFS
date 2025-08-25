use galleon_common::{config::GalleonConfig, error::Result, types::*};
use std::sync::Arc;

pub struct ClusterManager {
    config: GalleonConfig,
}

impl ClusterManager {
    pub async fn new(config: &GalleonConfig, _consensus: Arc<super::RaftConsensus>) -> Result<Self> {
        Ok(Self { config: config.clone() })
    }
    pub async fn start(&self) -> Result<()> { Ok(()) }
    pub async fn stop(&self) -> Result<()> { Ok(()) }
    pub async fn add_node(&self, _node_info: NodeInfo) -> Result<()> { Ok(()) }
    pub async fn remove_node(&self, _node_id: NodeId) -> Result<()> { Ok(()) }
    pub async fn get_all_nodes(&self) -> Result<Vec<NodeInfo>> { Ok(Vec::new()) }
}
