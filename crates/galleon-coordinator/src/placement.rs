//! Placement engine for intelligent chunk placement
use galleon_common::{config::GalleonConfig, error::Result, types::*};

pub struct PlacementEngine {
    config: GalleonConfig,
}

impl PlacementEngine {
    pub async fn new(config: &GalleonConfig) -> Result<Self> {
        Ok(Self { config: config.clone() })
    }
    pub async fn update_device_availability(&self, _device: Device) -> Result<()> { Ok(()) }
}
