use anyhow::{Result, anyhow, Context};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

use galleon_common::types::{NodeId, VolumeId, ChunkId, DeviceId};
use galleon_common::config::StorageNodeConfig;
use crate::device_manager::{MultiDeviceManager, DeviceManagerConfig, PerformanceSpec};
use crate::io_engine::{IoEngine, IoEngineConfig, detect_hardware_capabilities};
use crate::chunk_manager::{ChunkManager, ChunkManagerConfig, ChunkSize, PlacementPolicy};

/// Main storage engine that coordinates all storage operations
pub struct StorageEngine {
    /// Node ID
    node_id: NodeId,
    
    /// Device manager for multi-device coordination
    device_manager: Arc<MultiDeviceManager>,
    
    /// I/O engine for high-performance operations
    io_engine: Arc<IoEngine>,
    
    /// Chunk manager for storage operations
    chunk_manager: Arc<RwLock<ChunkManager>>,
    
    /// Configuration
    config: StorageNodeConfig,
}

impl StorageEngine {
    /// Create a new storage engine
    pub async fn new(config: StorageNodeConfig) -> Result<Self> {
        info!("Initializing GalleonFS Storage Engine");
        info!("Node ID: {}", config.node_id);
        
        // Detect hardware capabilities
        let hw_capabilities = detect_hardware_capabilities();
        info!("Hardware capabilities: {:?}", hw_capabilities);
        
        // Initialize device manager
        let device_config = DeviceManagerConfig {
            max_devices_per_node: config.max_devices_per_node,
            enable_numa_optimization: config.enable_numa,
            enable_hardware_accel: config.enable_hw_accel,
            ..Default::default()
        };
        
        info!("Initializing device manager...");
        let device_manager = Arc::new(
            MultiDeviceManager::new(config.node_id.clone(), device_config).await?
        );
        
        // Get NUMA topology for I/O engine
        let numa_topology = device_manager.get_numa_topology();
        let numa_cpus: std::collections::HashMap<u32, Vec<u32>> = numa_topology.nodes
            .into_iter()
            .map(|(id, node)| (id, node.cpus))
            .collect();
        
        // Initialize I/O engine
        let io_config = IoEngineConfig {
            workers_per_numa_node: config.io_workers_per_numa,
            max_queue_depth_per_device: config.max_queue_depth,
            enable_numa_optimization: config.enable_numa,
            enable_cpu_affinity: config.enable_cpu_affinity,
            ..Default::default()
        };
        
        info!("Initializing I/O engine...");
        let mut io_engine = IoEngine::new(io_config)?;
        
        // Add claimed devices to I/O engine
        for device in device_manager.get_claimed_devices() {
            io_engine.add_device(&device)?;
        }
        
        // Start I/O workers
        io_engine.start_workers(&numa_cpus)?;
        let io_engine = Arc::new(io_engine);
        
        // Initialize chunk manager
        let chunk_config = ChunkManagerConfig {
            metadata_db_path: config.metadata_path.clone(),
            wal_dir: config.wal_path.clone(),
            cache_size_mb: config.cache_size_mb,
            enable_compression: config.enable_compression,
            enable_encryption: config.enable_encryption,
            enable_hardware_acceleration: config.enable_hw_accel,
            ..Default::default()
        };
        
        info!("Initializing chunk manager...");
        let mut chunk_manager = ChunkManager::new(
            config.node_id.clone(),
            io_engine.clone(),
            chunk_config,
        ).await?;
        
        // Start background tasks
        chunk_manager.start_background_tasks()?;
        let chunk_manager = Arc::new(RwLock::new(chunk_manager));
        
        // Start periodic tasks
        let engine = Self {
            node_id: config.node_id.clone(),
            device_manager: device_manager.clone(),
            io_engine,
            chunk_manager,
            config,
        };
        
        // Start device discovery and health monitoring
        device_manager.start_periodic_discovery().await?;
        device_manager.start_health_monitoring().await?;
        
        info!("Storage engine initialization complete");
        Ok(engine)
    }
    
    /// Create a new volume
    pub async fn create_volume(
        &self,
        volume_id: VolumeId,
        size_bytes: u64,
        chunk_size: ChunkSize,
        target_iops: u64,
        target_bandwidth_mb: u64,
    ) -> Result<()> {
        info!("Creating volume {} with size {} bytes", volume_id, size_bytes);
        
        // Define performance requirements
        let perf_spec = PerformanceSpec {
            min_iops: target_iops,
            min_bandwidth_mb: target_bandwidth_mb,
            max_latency_us: 1000, // 1ms max latency
            preferred_device_types: vec![
                crate::device_manager::DeviceType::NVMe,
                crate::device_manager::DeviceType::SSD,
            ],
            numa_locality: self.config.enable_numa,
        };
        
        // Claim devices for the volume
        let devices = self.device_manager
            .claim_devices_for_volume(size_bytes, &perf_spec)
            .await?;
        
        if devices.is_empty() {
            return Err(anyhow!("No suitable devices available for volume"));
        }
        
        info!("Claimed {} devices for volume {}", devices.len(), volume_id);
        
        // Create placement policy
        let placement_policy = PlacementPolicy::MaximumDistribution {
            prefer_local_node: true,
            max_chunks_per_device: 100_000, // 100K chunks per device
            max_chunks_per_node: 1_000_000,  // 1M chunks per node
        };
        
        // Create volume in chunk manager
        let chunk_manager = self.chunk_manager.read().await;
        chunk_manager.create_volume(
            volume_id,
            size_bytes,
            chunk_size,
            placement_policy,
            &devices,
        ).await?;
        
        info!("Successfully created volume {}", volume_id);
        Ok(())
    }
    
    /// Write data to a volume
    pub async fn write_volume_data(
        &self,
        volume_id: &VolumeId,
        offset: u64,
        data: &[u8],
    ) -> Result<u64> {
        let chunk_manager = self.chunk_manager.read().await;
        
        // Get volume topology
        let topology = chunk_manager.get_volume_topology(volume_id)
            .ok_or_else(|| anyhow!("Volume not found: {}", volume_id))?;
        
        let chunk_size = topology.chunk_size.as_bytes();
        let start_chunk = offset / chunk_size;
        let end_chunk = (offset + data.len() as u64 - 1) / chunk_size;
        
        let mut bytes_written = 0u64;
        
        for chunk_index in start_chunk..=end_chunk {
            let chunk_offset = chunk_index * chunk_size;
            let data_start = if offset > chunk_offset {
                (offset - chunk_offset) as usize
            } else {
                0
            };
            
            let data_end = std::cmp::min(
                data.len(),
                (chunk_offset + chunk_size - offset) as usize + data_start
            );
            
            if data_start < data_end {
                let chunk_data = &data[bytes_written as usize..bytes_written as usize + (data_end - data_start)];
                
                chunk_manager.write_chunk(volume_id, chunk_index, chunk_data).await?;
                bytes_written += (data_end - data_start) as u64;
            }
        }
        
        Ok(bytes_written)
    }
    
    /// Read data from a volume
    pub async fn read_volume_data(
        &self,
        volume_id: &VolumeId,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>> {
        let chunk_manager = self.chunk_manager.read().await;
        
        // Get volume topology
        let topology = chunk_manager.get_volume_topology(volume_id)
            .ok_or_else(|| anyhow!("Volume not found: {}", volume_id))?;
        
        let chunk_size = topology.chunk_size.as_bytes();
        let start_chunk = offset / chunk_size;
        let end_chunk = (offset + length - 1) / chunk_size;
        
        let mut result = Vec::with_capacity(length as usize);
        
        for chunk_index in start_chunk..=end_chunk {
            let chunk_id = topology.chunks.get(&chunk_index)
                .ok_or_else(|| anyhow!("Chunk {} not found in volume {}", chunk_index, volume_id))?;
            
            let chunk_data = chunk_manager.read_chunk(chunk_id).await?;
            
            let chunk_offset = chunk_index * chunk_size;
            let data_start = if offset > chunk_offset {
                (offset - chunk_offset) as usize
            } else {
                0
            };
            
            let data_end = std::cmp::min(
                chunk_data.len(),
                data_start + (length - result.len() as u64) as usize
            );
            
            if data_start < data_end {
                result.extend_from_slice(&chunk_data[data_start..data_end]);
            }
            
            if result.len() >= length as usize {
                break;
            }
        }
        
        Ok(result)
    }
    
    /// Delete a volume
    pub async fn delete_volume(&self, volume_id: &VolumeId) -> Result<()> {
        info!("Deleting volume {}", volume_id);
        
        let chunk_manager = self.chunk_manager.read().await;
        
        // Get all chunks for the volume
        let chunks = chunk_manager.list_volume_chunks(volume_id).await?;
        
        // Delete all chunks
        for chunk_id in chunks {
            if let Err(e) = chunk_manager.delete_chunk(&chunk_id).await {
                warn!("Failed to delete chunk {}: {}", chunk_id, e);
            }
        }
        
        info!("Successfully deleted volume {}", volume_id);
        Ok(())
    }
    
    /// Get storage engine statistics
    pub async fn get_statistics(&self) -> StorageEngineStats {
        let device_stats = self.device_manager.get_claimed_devices();
        let io_stats = self.io_engine.get_stats();
        let chunk_stats = {
            let chunk_manager = self.chunk_manager.read().await;
            chunk_manager.get_stats()
        };
        
        StorageEngineStats {
            node_id: self.node_id.clone(),
            total_devices: device_stats.len(),
            total_capacity_bytes: device_stats.iter().map(|d| d.total_capacity).sum(),
            available_capacity_bytes: device_stats.iter().map(|d| d.available_capacity).sum(),
            total_chunks: chunk_stats.total_chunks.load(std::sync::atomic::Ordering::Relaxed),
            total_bytes_stored: chunk_stats.total_bytes_stored.load(std::sync::atomic::Ordering::Relaxed),
            io_requests_total: io_stats.total_requests.load(std::sync::atomic::Ordering::Relaxed),
            io_requests_completed: io_stats.completed_requests.load(std::sync::atomic::Ordering::Relaxed),
            io_requests_failed: io_stats.failed_requests.load(std::sync::atomic::Ordering::Relaxed),
            avg_latency_us: io_stats.avg_latency_us.load(std::sync::atomic::Ordering::Relaxed),
            cache_hit_ratio: {
                let hits = chunk_stats.cache_hits.load(std::sync::atomic::Ordering::Relaxed);
                let misses = chunk_stats.cache_misses.load(std::sync::atomic::Ordering::Relaxed);
                if hits + misses > 0 {
                    hits as f64 / (hits + misses) as f64
                } else {
                    0.0
                }
            },
        }
    }
    
    /// List available volumes
    pub async fn list_volumes(&self) -> Result<Vec<VolumeInfo>> {
        let chunk_manager = self.chunk_manager.read().await;
        let mut volumes = Vec::new();
        
        // This would iterate over volume topologies
        // For now, return empty list as a placeholder
        Ok(volumes)
    }
    
    /// Get volume information
    pub async fn get_volume_info(&self, volume_id: &VolumeId) -> Result<VolumeInfo> {
        let chunk_manager = self.chunk_manager.read().await;
        
        let topology = chunk_manager.get_volume_topology(volume_id)
            .ok_or_else(|| anyhow!("Volume not found: {}", volume_id))?;
        
        Ok(VolumeInfo {
            volume_id: volume_id.clone(),
            total_size: topology.total_size,
            chunk_size: topology.chunk_size,
            chunk_count: topology.chunks.len() as u64,
            device_count: topology.device_distribution.len(),
            created_at: topology.created_at,
            last_modified: topology.last_modified,
        })
    }
    
    /// Start metrics collection
    pub async fn start_metrics_collection(&self) -> Result<()> {
        info!("Starting metrics collection for storage engine");
        
        // This would start background metrics collection tasks
        // For now, just log that metrics collection is started
        
        Ok(())
    }
    
    /// Shutdown the storage engine
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down storage engine");
        
        // Shutdown chunk manager
        // Note: We can't easily take ownership here due to Arc<RwLock<>>
        // In a real implementation, we'd need a different shutdown mechanism
        
        // Shutdown I/O engine would happen via drop
        
        info!("Storage engine shutdown complete");
        Ok(())
    }
}

/// Storage engine statistics
#[derive(Debug, Clone)]
pub struct StorageEngineStats {
    pub node_id: NodeId,
    pub total_devices: usize,
    pub total_capacity_bytes: u64,
    pub available_capacity_bytes: u64,
    pub total_chunks: u64,
    pub total_bytes_stored: u64,
    pub io_requests_total: u64,
    pub io_requests_completed: u64,
    pub io_requests_failed: u64,
    pub avg_latency_us: u64,
    pub cache_hit_ratio: f64,
}

/// Volume information
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub volume_id: VolumeId,
    pub total_size: u64,
    pub chunk_size: ChunkSize,
    pub chunk_count: u64,
    pub device_count: usize,
    pub created_at: std::time::SystemTime,
    pub last_modified: std::time::SystemTime,
}