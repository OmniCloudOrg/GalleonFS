use crate::vfs::replication::types::*;

use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

impl PerformanceTracker {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            replication_metrics: Arc::new(RwLock::new(HashMap::new())),
            performance_history: Arc::new(RwLock::new(VecDeque::new())),
        })
    }
}