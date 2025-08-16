use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Instant;
use uuid::Uuid;

/// Performance tracking for adaptive optimization
pub struct PerformanceTracker {
    /// Replication performance metrics
    pub replication_metrics: Arc<RwLock<HashMap<Uuid, ReplicationMetrics>>>,
    /// Historical performance data
    pub performance_history: Arc<RwLock<VecDeque<PerformanceSnapshot>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationMetrics {
    /// Average replication latency
    pub avg_latency_ms: f64,
    /// Replication throughput (MB/s)
    pub throughput_mbps: f64,
    /// Success rate
    pub success_rate: f64,
    /// Error rate
    pub error_rate: f64,
    /// Bandwidth utilization
    pub bandwidth_utilization: f64,
}

#[derive(Debug, Clone)]
pub struct PerformanceSnapshot {
    /// Snapshot timestamp
    pub timestamp: Instant,
    /// Overall cluster performance
    pub cluster_metrics: ClusterPerformanceMetrics,
    /// Per-volume performance
    pub volume_metrics: HashMap<Uuid, VolumePerformanceMetrics>,
}

#[derive(Debug, Clone)]
pub struct ClusterPerformanceMetrics {
    /// Total cluster IOPS
    pub total_iops: u32,
    /// Total cluster throughput (MB/s)
    pub total_throughput_mbps: u32,
    /// Average cluster latency
    pub avg_latency_ms: f64,
    /// Cluster reliability score
    pub reliability_score: f64,
}

#[derive(Debug, Clone)]
pub struct VolumePerformanceMetrics {
    /// Volume IOPS
    pub iops: u32,
    /// Volume throughput (MB/s)
    pub throughput_mbps: u32,
    /// Volume latency
    pub avg_latency_ms: f64,
    /// Replication efficiency
    pub replication_efficiency: f64,
}

impl PerformanceTracker {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            replication_metrics: Arc::new(RwLock::new(HashMap::new())),
            performance_history: Arc::new(RwLock::new(VecDeque::new())),
        })
    }
}