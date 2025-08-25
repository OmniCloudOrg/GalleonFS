//! GalleonFS Coordinator - Control plane service for the distributed storage system
//!
//! The coordinator is responsible for:
//! - Cluster membership management
//! - Volume metadata management  
//! - Device topology tracking
//! - Placement decisions for multi-device volumes
//! - Leader election and consensus
//! - Health monitoring
//! - Data migration coordination

use galleon_common::{
    config::GalleonConfig,
    error::Result,
    types::*,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

mod consensus;
mod placement;
mod metadata;
mod api;
mod cluster;
mod health;
mod volume_manager;
mod topology;
mod migration;

pub use consensus::RaftConsensus;
pub use placement::PlacementEngine;
pub use metadata::MetadataStore;
pub use api::CoordinatorApi;
pub use cluster::ClusterManager;
pub use health::HealthMonitor;
pub use volume_manager::VolumeManager;
pub use topology::TopologyManager;
pub use migration::MigrationManager;

/// Main coordinator service
pub struct GalleonCoordinator {
    /// Configuration
    config: GalleonConfig,
    /// Consensus engine
    consensus: Arc<RaftConsensus>,
    /// Placement engine for device selection
    placement_engine: Arc<PlacementEngine>,
    /// Metadata store
    metadata_store: Arc<MetadataStore>,
    /// Cluster management
    cluster_manager: Arc<ClusterManager>,
    /// Health monitoring
    health_monitor: Arc<HealthMonitor>,
    /// Volume management
    volume_manager: Arc<VolumeManager>,
    /// Topology management
    topology_manager: Arc<TopologyManager>,
    /// Migration management
    migration_manager: Arc<MigrationManager>,
    /// gRPC API server
    api_server: Arc<CoordinatorApi>,
    /// Current cluster state
    cluster_state: Arc<RwLock<ClusterState>>,
}

impl GalleonCoordinator {
    /// Create a new coordinator instance
    pub async fn new(config: GalleonConfig) -> Result<Self> {
        info!("Initializing GalleonFS Coordinator");

        // Initialize consensus engine
        let consensus = Arc::new(RaftConsensus::new(&config).await?);
        
        // Initialize metadata store
        let metadata_store = Arc::new(MetadataStore::new(&config).await?);
        
        // Initialize placement engine
        let placement_engine = Arc::new(PlacementEngine::new(&config).await?);
        
        // Initialize cluster manager
        let cluster_manager = Arc::new(ClusterManager::new(&config, consensus.clone()).await?);
        
        // Initialize health monitor
        let health_monitor = Arc::new(HealthMonitor::new(&config).await?);
        
        // Initialize topology manager
        let topology_manager = Arc::new(TopologyManager::new(&config, metadata_store.clone()).await?);
        
        // Initialize volume manager
        let volume_manager = Arc::new(VolumeManager::new(
            &config,
            metadata_store.clone(),
            placement_engine.clone(),
            topology_manager.clone(),
        ).await?);
        
        // Initialize migration manager
        let migration_manager = Arc::new(MigrationManager::new(
            &config,
            metadata_store.clone(),
            cluster_manager.clone(),
        ).await?);
        
        // Initialize API server
        let api_server = Arc::new(CoordinatorApi::new(
            &config,
            cluster_manager.clone(),
            volume_manager.clone(),
            topology_manager.clone(),
            migration_manager.clone(),
        ).await?);

        let cluster_state = Arc::new(RwLock::new(ClusterState {
            active_nodes: Default::default(),
            device_health: Default::default(),
            load_distribution: Default::default(),
            network_congestion: Default::default(),
        }));

        Ok(Self {
            config,
            consensus,
            placement_engine,
            metadata_store,
            cluster_manager,
            health_monitor,
            volume_manager,
            topology_manager,
            migration_manager,
            api_server,
            cluster_state,
        })
    }

    /// Start the coordinator service
    pub async fn start(&self) -> Result<()> {
        info!("Starting GalleonFS Coordinator");

        // Start consensus engine
        self.consensus.start().await?;
        
        // Start cluster manager
        self.cluster_manager.start().await?;
        
        // Start health monitor
        self.health_monitor.start().await?;
        
        // Start topology manager
        self.topology_manager.start().await?;
        
        // Start migration manager
        self.migration_manager.start().await?;
        
        // Start API server
        self.api_server.start().await?;

        info!("GalleonFS Coordinator started successfully");
        Ok(())
    }

    /// Stop the coordinator service
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping GalleonFS Coordinator");

        // Stop services in reverse order
        self.api_server.stop().await?;
        self.migration_manager.stop().await?;
        self.topology_manager.stop().await?;
        self.health_monitor.stop().await?;
        self.cluster_manager.stop().await?;
        self.consensus.stop().await?;

        info!("GalleonFS Coordinator stopped");
        Ok(())
    }

    /// Check if this node is the cluster leader
    pub async fn is_leader(&self) -> bool {
        self.consensus.is_leader().await
    }

    /// Get current cluster state
    pub async fn get_cluster_state(&self) -> ClusterState {
        self.cluster_state.read().await.clone()
    }

    /// Get cluster statistics
    pub async fn get_cluster_stats(&self) -> Result<ClusterStats> {
        let nodes = self.cluster_manager.get_all_nodes().await?;
        let volumes = self.volume_manager.list_volumes().await?;
        let devices = self.topology_manager.get_all_devices().await?;

        let total_capacity = devices.iter().map(|d| d.capacity_bytes).sum();
        let used_capacity = devices.iter().map(|d| d.used_bytes).sum();
        let healthy_devices = devices.iter().filter(|d| d.status == DeviceStatus::Online).count();
        let degraded_devices = devices.iter().filter(|d| d.status == DeviceStatus::Degraded).count();
        let failed_devices = devices.iter().filter(|d| d.status == DeviceStatus::Failed).count();

        Ok(ClusterStats {
            total_nodes: nodes.len() as u32,
            active_nodes: nodes.iter().filter(|n| n.status == NodeStatus::Healthy).count() as u32,
            total_volumes: volumes.len() as u32,
            total_devices: devices.len() as u32,
            healthy_devices: healthy_devices as u32,
            degraded_devices: degraded_devices as u32,
            failed_devices: failed_devices as u32,
            total_capacity_bytes: total_capacity,
            used_capacity_bytes: used_capacity,
            utilization_percent: if total_capacity > 0 {
                (used_capacity as f64 / total_capacity as f64) * 100.0
            } else {
                0.0
            },
            last_updated: chrono::Utc::now(),
        })
    }

    /// Handle node join request
    pub async fn handle_node_join(&self, node_info: NodeInfo) -> Result<()> {
        info!("Node {} requesting to join cluster", node_info.id);
        
        // Validate node information
        if node_info.id == uuid::Uuid::nil() {
            return Err(galleon_common::error::GalleonError::InvalidInput(
                "Invalid node ID".to_string()
            ));
        }

        // Add node to cluster
        self.cluster_manager.add_node(node_info).await?;
        
        // Update cluster state
        let mut state = self.cluster_state.write().await;
        state.active_nodes.insert(node_info.id);
        
        info!("Node {} successfully joined cluster", node_info.id);
        Ok(())
    }

    /// Handle node leave request
    pub async fn handle_node_leave(&self, node_id: NodeId) -> Result<()> {
        info!("Node {} requesting to leave cluster", node_id);
        
        // Remove node from cluster
        self.cluster_manager.remove_node(node_id).await?;
        
        // Update cluster state
        let mut state = self.cluster_state.write().await;
        state.active_nodes.remove(&node_id);
        
        // Trigger data migration if necessary
        self.migration_manager.handle_node_removal(node_id).await?;
        
        info!("Node {} successfully left cluster", node_id);
        Ok(())
    }

    /// Handle device registration
    pub async fn handle_device_registration(&self, device: Device) -> Result<()> {
        info!("Registering device {} from node {}", device.id, device.node_id);
        
        // Validate device
        if device.capacity_bytes == 0 {
            return Err(galleon_common::error::GalleonError::InvalidInput(
                "Device capacity cannot be zero".to_string()
            ));
        }

        // Register device with topology manager
        self.topology_manager.register_device(device.clone()).await?;
        
        // Update placement engine
        self.placement_engine.update_device_availability(device).await?;
        
        info!("Device {} registered successfully", device.id);
        Ok(())
    }

    /// Create a new volume
    pub async fn create_volume(&self, request: CreateVolumeRequest) -> Result<Volume> {
        info!("Creating volume: {:?}", request);
        
        if !self.is_leader().await {
            return Err(galleon_common::error::GalleonError::ClusterError(
                "Only cluster leader can create volumes".to_string()
            ));
        }

        self.volume_manager.create_volume(request).await
    }

    /// Delete a volume
    pub async fn delete_volume(&self, volume_id: VolumeId) -> Result<()> {
        info!("Deleting volume {}", volume_id);
        
        if !self.is_leader().await {
            return Err(galleon_common::error::GalleonError::ClusterError(
                "Only cluster leader can delete volumes".to_string()
            ));
        }

        self.volume_manager.delete_volume(volume_id).await
    }

    /// Get volume information
    pub async fn get_volume(&self, volume_id: VolumeId) -> Result<Volume> {
        self.volume_manager.get_volume(volume_id).await
    }

    /// List all volumes
    pub async fn list_volumes(&self) -> Result<Vec<Volume>> {
        self.volume_manager.list_volumes().await
    }

    /// Update volume configuration
    pub async fn update_volume(&self, volume_id: VolumeId, update: VolumeUpdate) -> Result<Volume> {
        if !self.is_leader().await {
            return Err(galleon_common::error::GalleonError::ClusterError(
                "Only cluster leader can update volumes".to_string()
            ));
        }

        self.volume_manager.update_volume(volume_id, update).await
    }
}

/// Request to create a new volume
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateVolumeRequest {
    pub name: Option<String>,
    pub size_bytes: u64,
    pub volume_type: VolumeType,
    pub storage_class: String,
    pub replication_factor: Option<u8>,
    pub encryption: Option<EncryptionConfig>,
    pub compression: Option<CompressionConfig>,
    pub placement_requirements: Option<PlacementRequirements>,
}

/// Volume update request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VolumeUpdate {
    pub name: Option<String>,
    pub size_bytes: Option<u64>,
    pub replication_factor: Option<u8>,
    pub storage_class: Option<String>,
}

/// Placement requirements for volume creation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlacementRequirements {
    pub preferred_zones: Vec<String>,
    pub required_zones: Vec<String>,
    pub anti_affinity_zones: Vec<String>,
    pub performance_tier: Option<PerformanceTier>,
    pub max_latency_ms: Option<f32>,
    pub min_iops: Option<u32>,
}

/// Cluster statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClusterStats {
    pub total_nodes: u32,
    pub active_nodes: u32,
    pub total_volumes: u32,
    pub total_devices: u32,
    pub healthy_devices: u32,
    pub degraded_devices: u32,
    pub failed_devices: u32,
    pub total_capacity_bytes: u64,
    pub used_capacity_bytes: u64,
    pub utilization_percent: f64,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}