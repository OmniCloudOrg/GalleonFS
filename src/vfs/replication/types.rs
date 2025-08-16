use serde::{Deserialize, Serialize};
use uuid::Uuid;

// General types used across modules

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub enum StorageTier {
    NVMe,      // Ultra-fast NVMe SSDs
    SSD,       // Standard SSDs
    HDD,       // High-capacity HDDs
    Memory,    // In-memory caching
    Archive,   // Long-term archive storage
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStatus {
    /// Volume ID
    pub volume_id: Uuid,
    /// Current replication state
    pub state: ReplicationState,
    /// Active replicas
    pub active_replicas: u8,
    /// Target replicas
    pub target_replicas: u8,
    /// Replication health score
    pub health_score: f64,
    /// Performance metrics
    pub performance: crate::vfs::replication::tracker::ReplicationMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplicationState {
    /// All replicas healthy and consistent
    Healthy,
    /// Some replicas are syncing
    Syncing,
    /// Replicas are inconsistent
    Inconsistent,
    /// Replication is degraded but functional
    Degraded,
    /// Replication is failing
    Failed,
}
