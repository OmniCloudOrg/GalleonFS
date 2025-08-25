use galleon_common::{config::GalleonConfig, error::Result};

pub struct MetadataStore {
    config: GalleonConfig,
}

impl MetadataStore {
    pub async fn new(config: &GalleonConfig) -> Result<Self> {
        Ok(Self { config: config.clone() })
    }
}
