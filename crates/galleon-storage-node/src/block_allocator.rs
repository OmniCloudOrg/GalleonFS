//! Block allocator for managing device space allocation

use anyhow::Result;
use galleon_common::types::DeviceId;

pub struct BlockAllocator {
    device_id: DeviceId,
}

impl BlockAllocator {
    pub fn new(device_id: DeviceId) -> Self {
        Self { device_id }
    }
    
    pub async fn allocate(&self, size: u64) -> Result<u64> {
        // TODO: Implement block allocation
        Ok(0)
    }
    
    pub async fn deallocate(&self, offset: u64, size: u64) -> Result<()> {
        // TODO: Implement block deallocation
        Ok(())
    }
}