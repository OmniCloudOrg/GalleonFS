//! Metrics collection and performance monitoring for GalleonFS

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use chrono::{DateTime, Utc};
use crate::types::*;
use crate::error::Result;

/// Metrics collector for GalleonFS
pub struct MetricsCollector {
    /// Node metrics
    node_metrics: Arc<RwLock<NodeMetrics>>,
    /// Volume metrics by volume ID
    volume_metrics: Arc<RwLock<HashMap<VolumeId, VolumeMetrics>>>,
    /// Device metrics by device ID
    device_metrics: Arc<RwLock<HashMap<DeviceId, DeviceMetrics>>>,
    /// Cluster metrics
    cluster_metrics: Arc<RwLock<ClusterMetrics>>,
    /// Performance counters
    performance_counters: Arc<RwLock<PerformanceCounters>>,
}

/// Device-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceMetrics {
    /// Device ID
    pub device_id: DeviceId,
    /// Read IOPS
    pub read_iops: f64,
    /// Write IOPS
    pub write_iops: f64,
    /// Read throughput in bytes per second
    pub read_throughput_bps: f64,
    /// Write throughput in bytes per second
    pub write_throughput_bps: f64,
    /// Read latency in milliseconds
    pub read_latency_ms: f64,
    /// Write latency in milliseconds
    pub write_latency_ms: f64,
    /// Queue depth
    pub queue_depth: u32,
    /// Utilization percentage (0.0 to 1.0)
    pub utilization_percent: f64,
    /// Error rate
    pub error_rate: f64,
    /// Temperature in Celsius
    pub temperature_celsius: Option<f32>,
    /// SMART attributes
    pub smart_attributes: SmartAttributes,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

/// SMART attributes for device health monitoring
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SmartAttributes {
    /// Power-on hours
    pub power_on_hours: Option<u64>,
    /// Power cycle count
    pub power_cycle_count: Option<u64>,
    /// Reallocated sector count
    pub reallocated_sectors: Option<u64>,
    /// Current pending sectors
    pub pending_sectors: Option<u64>,
    /// Uncorrectable sector count
    pub uncorrectable_sectors: Option<u64>,
    /// Temperature
    pub temperature: Option<f32>,
    /// Wear leveling count (for SSDs)
    pub wear_leveling: Option<u64>,
    /// Program/erase cycles (for SSDs)
    pub program_erase_cycles: Option<u64>,
}

/// Cluster-wide metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClusterMetrics {
    /// Total cluster capacity in bytes
    pub total_capacity_bytes: u64,
    /// Total used capacity in bytes
    pub used_capacity_bytes: u64,
    /// Number of active nodes
    pub active_nodes: u32,
    /// Number of healthy devices
    pub healthy_devices: u32,
    /// Number of degraded devices
    pub degraded_devices: u32,
    /// Number of failed devices
    pub failed_devices: u32,
    /// Total volumes
    pub total_volumes: u32,
    /// Cluster-wide read IOPS
    pub cluster_read_iops: f64,
    /// Cluster-wide write IOPS
    pub cluster_write_iops: f64,
    /// Cluster-wide read throughput
    pub cluster_read_throughput_bps: f64,
    /// Cluster-wide write throughput
    pub cluster_write_throughput_bps: f64,
    /// Average cluster latency
    pub avg_latency_ms: f64,
    /// Replication lag in milliseconds
    pub replication_lag_ms: f64,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

/// Performance counters for detailed analysis
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerformanceCounters {
    /// I/O operation counters
    pub io_counters: IoCounters,
    /// Network operation counters
    pub network_counters: NetworkCounters,
    /// Cache performance counters
    pub cache_counters: CacheCounters,
    /// Error counters
    pub error_counters: ErrorCounters,
}

/// I/O operation counters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IoCounters {
    /// Total read operations
    pub total_reads: u64,
    /// Total write operations
    pub total_writes: u64,
    /// Total bytes read
    pub total_bytes_read: u64,
    /// Total bytes written
    pub total_bytes_written: u64,
    /// Read operation duration histogram
    pub read_duration_histogram: LatencyHistogram,
    /// Write operation duration histogram
    pub write_duration_histogram: LatencyHistogram,
    /// Queue depth histogram
    pub queue_depth_histogram: HashMap<u32, u64>,
}

/// Network operation counters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkCounters {
    /// Total network requests sent
    pub requests_sent: u64,
    /// Total network requests received
    pub requests_received: u64,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Network latency histogram
    pub network_latency_histogram: LatencyHistogram,
    /// Connection pool metrics
    pub connection_pool_size: u32,
    /// Active connections
    pub active_connections: u32,
}

/// Cache performance counters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheCounters {
    /// Cache hits
    pub cache_hits: u64,
    /// Cache misses
    pub cache_misses: u64,
    /// Cache evictions
    pub cache_evictions: u64,
    /// Cache size in bytes
    pub cache_size_bytes: u64,
    /// Cache utilization percentage
    pub cache_utilization_percent: f64,
}

/// Error counters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ErrorCounters {
    /// I/O errors
    pub io_errors: u64,
    /// Network errors
    pub network_errors: u64,
    /// Replication errors
    pub replication_errors: u64,
    /// Checksum errors
    pub checksum_errors: u64,
    /// Device errors
    pub device_errors: u64,
    /// Timeout errors
    pub timeout_errors: u64,
}

/// Latency histogram for performance analysis
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LatencyHistogram {
    /// Buckets for latency ranges (in microseconds)
    pub buckets: HashMap<String, u64>,
    /// Total count
    pub total_count: u64,
    /// Sum of all latencies
    pub total_sum_microseconds: u64,
}

impl LatencyHistogram {
    /// Create a new latency histogram with standard buckets
    pub fn new() -> Self {
        let mut buckets = HashMap::new();
        // Standard latency buckets in microseconds
        let bucket_ranges = vec![
            "0-10", "10-50", "50-100", "100-500", "500-1000",
            "1000-5000", "5000-10000", "10000-50000", "50000+",
        ];
        
        for range in bucket_ranges {
            buckets.insert(range.to_string(), 0);
        }

        Self {
            buckets,
            total_count: 0,
            total_sum_microseconds: 0,
        }
    }

    /// Record a latency measurement
    pub fn record(&mut self, latency_microseconds: u64) {
        self.total_count += 1;
        self.total_sum_microseconds += latency_microseconds;

        let bucket = match latency_microseconds {
            0..=10 => "0-10",
            11..=50 => "10-50",
            51..=100 => "50-100",
            101..=500 => "100-500",
            501..=1000 => "500-1000",
            1001..=5000 => "1000-5000",
            5001..=10000 => "5000-10000",
            10001..=50000 => "10000-50000",
            _ => "50000+",
        };

        *self.buckets.entry(bucket.to_string()).or_insert(0) += 1;
    }

    /// Calculate average latency
    pub fn average_latency_microseconds(&self) -> f64 {
        if self.total_count == 0 {
            0.0
        } else {
            self.total_sum_microseconds as f64 / self.total_count as f64
        }
    }

    /// Calculate percentile latency
    pub fn percentile_latency(&self, percentile: f64) -> Option<u64> {
        if self.total_count == 0 || percentile < 0.0 || percentile > 1.0 {
            return None;
        }

        let target_count = (self.total_count as f64 * percentile) as u64;
        let mut cumulative_count = 0;

        // Ordered bucket keys for percentile calculation
        let ordered_buckets = vec![
            "0-10", "10-50", "50-100", "100-500", "500-1000",
            "1000-5000", "5000-10000", "10000-50000", "50000+",
        ];

        for bucket in ordered_buckets {
            cumulative_count += self.buckets.get(bucket).unwrap_or(&0);
            if cumulative_count >= target_count {
                // Return the upper bound of the bucket as an approximation
                return Some(match bucket {
                    "0-10" => 10,
                    "10-50" => 50,
                    "50-100" => 100,
                    "100-500" => 500,
                    "500-1000" => 1000,
                    "1000-5000" => 5000,
                    "5000-10000" => 10000,
                    "10000-50000" => 50000,
                    "50000+" => 100000, // Estimate for the highest bucket
                    _ => 0,
                });
            }
        }

        None
    }
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            node_metrics: Arc::new(RwLock::new(NodeMetrics::default())),
            volume_metrics: Arc::new(RwLock::new(HashMap::new())),
            device_metrics: Arc::new(RwLock::new(HashMap::new())),
            cluster_metrics: Arc::new(RwLock::new(ClusterMetrics::default())),
            performance_counters: Arc::new(RwLock::new(PerformanceCounters::default())),
        }
    }

    /// Update node metrics
    pub fn update_node_metrics(&self, metrics: NodeMetrics) {
        *self.node_metrics.write() = metrics;
    }

    /// Update volume metrics
    pub fn update_volume_metrics(&self, volume_id: VolumeId, metrics: VolumeMetrics) {
        self.volume_metrics.write().insert(volume_id, metrics);
    }

    /// Update device metrics
    pub fn update_device_metrics(&self, device_id: DeviceId, metrics: DeviceMetrics) {
        self.device_metrics.write().insert(device_id, metrics);
    }

    /// Update cluster metrics
    pub fn update_cluster_metrics(&self, metrics: ClusterMetrics) {
        *self.cluster_metrics.write() = metrics;
    }

    /// Record I/O operation
    pub fn record_io_operation(&self, operation: IoOperation) {
        let mut counters = self.performance_counters.write();
        
        match operation.operation_type {
            IoOperationType::Read => {
                counters.io_counters.total_reads += 1;
                counters.io_counters.total_bytes_read += operation.bytes;
                counters.io_counters.read_duration_histogram
                    .record(operation.duration_microseconds);
            }
            IoOperationType::Write => {
                counters.io_counters.total_writes += 1;
                counters.io_counters.total_bytes_written += operation.bytes;
                counters.io_counters.write_duration_histogram
                    .record(operation.duration_microseconds);
            }
        }

        // Record queue depth
        *counters.io_counters.queue_depth_histogram
            .entry(operation.queue_depth)
            .or_insert(0) += 1;
    }

    /// Record network operation
    pub fn record_network_operation(&self, operation: NetworkOperation) {
        let mut counters = self.performance_counters.write();
        
        match operation.direction {
            NetworkDirection::Send => {
                counters.network_counters.requests_sent += 1;
                counters.network_counters.bytes_sent += operation.bytes;
            }
            NetworkDirection::Receive => {
                counters.network_counters.requests_received += 1;
                counters.network_counters.bytes_received += operation.bytes;
            }
        }

        counters.network_counters.network_latency_histogram
            .record(operation.latency_microseconds);
    }

    /// Record cache operation
    pub fn record_cache_operation(&self, operation: CacheOperation) {
        let mut counters = self.performance_counters.write();
        
        match operation.result {
            CacheResult::Hit => counters.cache_counters.cache_hits += 1,
            CacheResult::Miss => counters.cache_counters.cache_misses += 1,
        }

        if operation.eviction_occurred {
            counters.cache_counters.cache_evictions += 1;
        }
    }

    /// Record error
    pub fn record_error(&self, error_type: ErrorType) {
        let mut counters = self.performance_counters.write();
        
        match error_type {
            ErrorType::IoError => counters.error_counters.io_errors += 1,
            ErrorType::NetworkError => counters.error_counters.network_errors += 1,
            ErrorType::ReplicationError => counters.error_counters.replication_errors += 1,
            ErrorType::ChecksumError => counters.error_counters.checksum_errors += 1,
            ErrorType::DeviceError => counters.error_counters.device_errors += 1,
            ErrorType::TimeoutError => counters.error_counters.timeout_errors += 1,
        }
    }

    /// Get current node metrics
    pub fn get_node_metrics(&self) -> NodeMetrics {
        self.node_metrics.read().clone()
    }

    /// Get volume metrics
    pub fn get_volume_metrics(&self, volume_id: &VolumeId) -> Option<VolumeMetrics> {
        self.volume_metrics.read().get(volume_id).cloned()
    }

    /// Get all volume metrics
    pub fn get_all_volume_metrics(&self) -> HashMap<VolumeId, VolumeMetrics> {
        self.volume_metrics.read().clone()
    }

    /// Get device metrics
    pub fn get_device_metrics(&self, device_id: &DeviceId) -> Option<DeviceMetrics> {
        self.device_metrics.read().get(device_id).cloned()
    }

    /// Get all device metrics
    pub fn get_all_device_metrics(&self) -> HashMap<DeviceId, DeviceMetrics> {
        self.device_metrics.read().clone()
    }

    /// Get cluster metrics
    pub fn get_cluster_metrics(&self) -> ClusterMetrics {
        self.cluster_metrics.read().clone()
    }

    /// Get performance counters
    pub fn get_performance_counters(&self) -> PerformanceCounters {
        self.performance_counters.read().clone()
    }

    /// Generate metrics summary
    pub fn generate_summary(&self) -> MetricsSummary {
        let node_metrics = self.get_node_metrics();
        let cluster_metrics = self.get_cluster_metrics();
        let volume_metrics = self.get_all_volume_metrics();
        let device_metrics = self.get_all_device_metrics();
        let performance_counters = self.get_performance_counters();

        MetricsSummary {
            timestamp: Utc::now(),
            node_metrics,
            cluster_metrics,
            volume_count: volume_metrics.len() as u32,
            device_count: device_metrics.len() as u32,
            total_read_iops: volume_metrics.values().map(|m| m.read_iops).sum(),
            total_write_iops: volume_metrics.values().map(|m| m.write_iops).sum(),
            avg_read_latency_ms: {
                let latencies: Vec<f64> = volume_metrics.values().map(|m| m.read_latency_ms).collect();
                if latencies.is_empty() { 0.0 } else { latencies.iter().sum::<f64>() / latencies.len() as f64 }
            },
            avg_write_latency_ms: {
                let latencies: Vec<f64> = volume_metrics.values().map(|m| m.write_latency_ms).collect();
                if latencies.is_empty() { 0.0 } else { latencies.iter().sum::<f64>() / latencies.len() as f64 }
            },
            total_errors: performance_counters.error_counters.io_errors +
                          performance_counters.error_counters.network_errors +
                          performance_counters.error_counters.replication_errors +
                          performance_counters.error_counters.checksum_errors +
                          performance_counters.error_counters.device_errors +
                          performance_counters.error_counters.timeout_errors,
            cache_hit_ratio: {
                let hits = performance_counters.cache_counters.cache_hits;
                let misses = performance_counters.cache_counters.cache_misses;
                if hits + misses == 0 { 0.0 } else { hits as f64 / (hits + misses) as f64 }
            },
        }
    }
}

/// I/O operation for metrics recording
#[derive(Debug, Clone)]
pub struct IoOperation {
    pub operation_type: IoOperationType,
    pub bytes: u64,
    pub duration_microseconds: u64,
    pub queue_depth: u32,
}

/// Type of I/O operation
#[derive(Debug, Clone, PartialEq)]
pub enum IoOperationType {
    Read,
    Write,
}

/// Network operation for metrics recording
#[derive(Debug, Clone)]
pub struct NetworkOperation {
    pub direction: NetworkDirection,
    pub bytes: u64,
    pub latency_microseconds: u64,
}

/// Network operation direction
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkDirection {
    Send,
    Receive,
}

/// Cache operation for metrics recording
#[derive(Debug, Clone)]
pub struct CacheOperation {
    pub result: CacheResult,
    pub eviction_occurred: bool,
}

/// Cache operation result
#[derive(Debug, Clone, PartialEq)]
pub enum CacheResult {
    Hit,
    Miss,
}

/// Error types for metrics recording
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorType {
    IoError,
    NetworkError,
    ReplicationError,
    ChecksumError,
    DeviceError,
    TimeoutError,
}

/// High-level metrics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub timestamp: DateTime<Utc>,
    pub node_metrics: NodeMetrics,
    pub cluster_metrics: ClusterMetrics,
    pub volume_count: u32,
    pub device_count: u32,
    pub total_read_iops: f64,
    pub total_write_iops: f64,
    pub avg_read_latency_ms: f64,
    pub avg_write_latency_ms: f64,
    pub total_errors: u64,
    pub cache_hit_ratio: f64,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}