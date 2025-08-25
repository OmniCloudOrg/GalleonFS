use galleon_common::{config::NodeConfig, error::GalleonResult};

/// High-performance FUSE filesystem for GalleonFS volumes
#[derive(Debug)]
pub struct GalleonFuseFS {
    _config: NodeConfig,
}

impl GalleonFuseFS {
    pub async fn new(config: &NodeConfig) -> GalleonResult<Self> {
        Ok(Self {
            _config: config.clone(),
        })
    }
}