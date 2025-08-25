use galleon_common::{config::GalleonConfig, error::Result};

pub struct HealthMonitor {
    config: GalleonConfig,
}

impl HealthMonitor {
    pub async fn new(config: &GalleonConfig) -> Result<Self> {
        Ok(Self { config: config.clone() })
    }
    pub async fn start(&self) -> Result<()> { Ok(()) }
    pub async fn stop(&self) -> Result<()> { Ok(()) }
}
