use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::{VfsVolume, PlacementPolicy};

/// Docker Swarm-like simple cluster management for bootstrapping
/// This provides the foundation for more complex internal replication
#[derive(Clone)]
pub struct ClusterManager {
    /// This node's ID
    node_id: Uuid,
    /// Known peer nodes (Docker Swarm-like discovery)
    peers: Arc<RwLock<HashMap<Uuid, ClusterNode>>>,
    /// Cluster membership state
    cluster_state: Arc<RwLock<ClusterState>>,
    /// Network topology awareness
    topology: Arc<RwLock<NetworkTopology>>,
    /// Simple peer addresses for bootstrapping
    bootstrap_peers: Arc<Mutex<Vec<SocketAddr>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNode {
    pub id: Uuid,
    pub address: SocketAddr,
    pub zone: Option<String>,
    pub labels: HashMap<String, String>,
    pub capacity: NodeCapacity,
    pub status: NodeStatus,
    pub last_heartbeat: u64,
    pub join_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapacity {
    pub total_storage_bytes: u64,
    pub available_storage_bytes: u64,
    pub cpu_cores: u32,
    pub memory_bytes: u64,
    pub network_bandwidth_mbps: u32,
    pub iops_capability: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is healthy and accepting work
    Active,
    /// Node is healthy but not accepting new work
    Draining,
    /// Node is unreachable but may recover
    Unreachable,
    /// Node has left the cluster permanently
    Left,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterState {
    /// Unique cluster ID (first node creates this)
    pub cluster_id: Uuid,
    /// Leader node for coordination
    pub leader_id: Option<Uuid>,
    /// Current cluster membership
    pub member_count: u32,
    /// Zones represented in cluster
    pub zones: HashSet<String>,
    /// Cluster formation time
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTopology {
    /// Zone assignments for nodes
    pub zone_map: HashMap<Uuid, String>,
    /// Network latency matrix between zones
    pub zone_latency: HashMap<(String, String), Duration>,
    /// Bandwidth capabilities between zones
    pub zone_bandwidth: HashMap<(String, String), u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    pub cluster_id: Uuid,
    pub node_id: Uuid,
    pub is_leader: bool,
    pub total_nodes: u32,
    pub active_nodes: u32,
    pub zones: Vec<String>,
    pub total_capacity_bytes: u64,
    pub available_capacity_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClusterMessage {
    /// Join request from new node
    JoinRequest {
        node_id: Uuid,
        node_info: ClusterNode,
    },
    /// Join response with cluster state
    JoinResponse {
        accepted: bool,
        cluster_state: Option<ClusterState>,
        peer_list: Vec<ClusterNode>,
    },
    /// Heartbeat to maintain membership
    Heartbeat {
        node_id: Uuid,
        timestamp: u64,
        capacity: NodeCapacity,
    },
    /// Leader election message
    LeaderElection {
        candidate_id: Uuid,
        term: u64,
    },
    /// Volume registration for distributed placement
    VolumeRegistration {
        volume: VfsVolume,
        placement_nodes: Vec<Uuid>,
    },
}

impl ClusterManager {
    pub async fn new(node_id: Uuid) -> Result<Self> {
        info!("🤝 Initializing Docker Swarm-like cluster manager for node: {}", node_id);

        // Initialize with this node as the first member
        let mut initial_peers = HashMap::new();
        let self_node = ClusterNode {
            id: node_id,
            address: "127.0.0.1:8080".parse()?, // Will be updated when binding
            zone: std::env::var("GALLEONFS_ZONE").ok(),
            labels: Self::detect_node_labels().await?,
            capacity: Self::detect_node_capacity().await?,
            status: NodeStatus::Active,
            last_heartbeat: Self::current_timestamp(),
            join_time: Self::current_timestamp(),
        };
        initial_peers.insert(node_id, self_node);

        let cluster_state = ClusterState {
            cluster_id: Uuid::new_v4(),
            leader_id: Some(node_id), // Start as leader of single-node cluster
            member_count: 1,
            zones: HashSet::new(),
            created_at: Self::current_timestamp(),
        };

        info!("✅ Cluster manager initialized as single-node cluster");

        Ok(Self {
            node_id,
            peers: Arc::new(RwLock::new(initial_peers)),
            cluster_state: Arc::new(RwLock::new(cluster_state)),
            topology: Arc::new(RwLock::new(NetworkTopology {
                zone_map: HashMap::new(),
                zone_latency: HashMap::new(),
                zone_bandwidth: HashMap::new(),
            })),
            bootstrap_peers: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Docker Swarm-like join: provide peer addresses to bootstrap into existing cluster
    pub async fn join_cluster(&self, peer_addresses: Vec<SocketAddr>) -> Result<()> {
        info!("🔗 Attempting to join cluster via peers: {:?}", peer_addresses);

        {
            let mut bootstrap = self.bootstrap_peers.lock().await;
            *bootstrap = peer_addresses.clone();
        }

        // Try to connect to any available peer (Docker Swarm style)
        for peer_addr in peer_addresses {
            match self.attempt_join_via_peer(peer_addr).await {
                Ok(_) => {
                    info!("✅ Successfully joined cluster via peer: {}", peer_addr);
                    self.start_cluster_maintenance().await;
                    return Ok(());
                }
                Err(e) => {
                    warn!("❌ Failed to join via peer {}: {}", peer_addr, e);
                    continue;
                }
            }
        }

        Err(anyhow::anyhow!("Failed to join cluster via any provided peer"))
    }

    async fn attempt_join_via_peer(&self, peer_addr: SocketAddr) -> Result<()> {
        debug!("🤝 Attempting join via peer: {}", peer_addr);

        // Connect to peer
        let mut stream = timeout(Duration::from_secs(10), TcpStream::connect(peer_addr)).await??;

        // Create join request
        let self_node = {
            let peers = self.peers.read().await;
            peers.get(&self.node_id).unwrap().clone()
        };

        let join_request = ClusterMessage::JoinRequest {
            node_id: self.node_id,
            node_info: self_node,
        };

        // Send join request
        self.send_cluster_message(&mut stream, &join_request).await?;

        // Wait for response
        let response = self.receive_cluster_message(&mut stream).await?;

        match response {
            ClusterMessage::JoinResponse { accepted: true, cluster_state: Some(state), peer_list } => {
                info!("🎉 Join accepted! Updating cluster state");

                // Update our cluster state
                {
                    let mut current_state = self.cluster_state.write().await;
                    *current_state = state;
                }

                // Update peer list
                {
                    let mut peers = self.peers.write().await;
                    peers.clear();
                    for peer in peer_list {
                        peers.insert(peer.id, peer);
                    }
                }

                Ok(())
            }
            ClusterMessage::JoinResponse { accepted: false, .. } => {
                Err(anyhow::anyhow!("Join request rejected by peer"))
            }
            _ => Err(anyhow::anyhow!("Unexpected response to join request")),
        }
    }

    /// Start background maintenance tasks (heartbeats, leader election, etc.)
    async fn start_cluster_maintenance(&self) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut heartbeat_interval = interval(Duration::from_secs(30));
            let mut cleanup_interval = interval(Duration::from_secs(60));

            loop {
                tokio::select! {
                    _ = heartbeat_interval.tick() => {
                        if let Err(e) = manager.send_heartbeats().await {
                            error!("❌ Heartbeat failed: {}", e);
                        }
                    }
                    _ = cleanup_interval.tick() => {
                        if let Err(e) = manager.cleanup_unreachable_nodes().await {
                            error!("❌ Node cleanup failed: {}", e);
                        }
                    }
                }
            }
        });
    }

    async fn send_heartbeats(&self) -> Result<()> {
        let capacity = Self::detect_node_capacity().await?;
        let heartbeat = ClusterMessage::Heartbeat {
            node_id: self.node_id,
            timestamp: Self::current_timestamp(),
            capacity,
        };

        let peer_addresses: Vec<SocketAddr> = {
            let peers = self.peers.read().await;
            peers.values()
                .filter(|p| p.id != self.node_id && p.status == NodeStatus::Active)
                .map(|p| p.address)
                .collect()
        };

        for addr in peer_addresses {
            let heartbeat = heartbeat.clone();
            let _ = tokio::spawn(async move {
                if let Ok(mut stream) = TcpStream::connect(addr).await {
                    let _ = Self::send_cluster_message_static(&mut stream, &heartbeat).await;
                }
            });
        }

        Ok(())
    }

    async fn cleanup_unreachable_nodes(&self) -> Result<()> {
        let now = Self::current_timestamp();
        let timeout_threshold = 180; // 3 minutes

        let mut nodes_to_remove = Vec::new();
        {
            let peers = self.peers.read().await;
            for (id, node) in peers.iter() {
                if *id != self.node_id && (now - node.last_heartbeat) > timeout_threshold {
                    nodes_to_remove.push(*id);
                }
            }
        }

        if !nodes_to_remove.is_empty() {
            warn!("🧹 Cleaning up {} unreachable nodes", nodes_to_remove.len());
            let mut peers = self.peers.write().await;
            let mut state = self.cluster_state.write().await;
            
            for node_id in nodes_to_remove {
                peers.remove(&node_id);
                state.member_count = state.member_count.saturating_sub(1);
            }
        }

        Ok(())
    }

    /// Register a volume for distributed placement (foundation for complex replication)
    pub async fn register_volume(&self, volume: &VfsVolume) -> Result<()> {
        info!("📋 Registering volume for cluster placement: {} ({})", 
              volume.name, volume.id);

        // Simple placement for Docker Swarm-like behavior
        let placement_nodes = self.select_placement_nodes(volume).await?;
        
        debug!("📍 Selected placement nodes: {:?}", placement_nodes);

        // This is where the complex replication system will build upon this simple foundation
        let registration = ClusterMessage::VolumeRegistration {
            volume: volume.clone(),
            placement_nodes: placement_nodes.clone(),
        };

        // Broadcast to relevant nodes
        self.broadcast_to_nodes(&registration, &placement_nodes).await?;

        Ok(())
    }

    /// Select nodes for volume placement based on policy
    async fn select_placement_nodes(&self, volume: &VfsVolume) -> Result<Vec<Uuid>> {
        let peers = self.peers.read().await;
        let active_peers: Vec<_> = peers.values()
            .filter(|p| p.status == NodeStatus::Active)
            .collect();

        if active_peers.is_empty() {
            return Err(anyhow::anyhow!("No active nodes available for placement"));
        }

        let selected = match &volume.metadata.placement_policy {
            PlacementPolicy::Spread => {
                // Select all available nodes for maximum spread
                active_peers.iter().map(|p| p.id).collect()
            }
            PlacementPolicy::Balanced => {
                // Select nodes with best capacity balance
                let mut sorted_peers = active_peers.clone();
                sorted_peers.sort_by_key(|p| p.capacity.available_storage_bytes);
                sorted_peers.into_iter()
                    .take(volume.replication_factor as usize)
                    .map(|p| p.id)
                    .collect()
            }
            PlacementPolicy::ZoneAware => {
                // Distribute across zones if available
                let mut zone_nodes: HashMap<String, Vec<Uuid>> = HashMap::new();
                for peer in &active_peers {
                    if let Some(zone) = &peer.zone {
                        zone_nodes.entry(zone.clone()).or_default().push(peer.id);
                    }
                }

                let mut selected = Vec::new();
                let zones: Vec<_> = zone_nodes.keys().cloned().collect();
                
                for i in 0..volume.replication_factor as usize {
                    if let Some(zone) = zones.get(i % zones.len()) {
                        if let Some(nodes) = zone_nodes.get(zone) {
                            if let Some(node_id) = nodes.get(i / zones.len()) {
                                selected.push(*node_id);
                            }
                        }
                    }
                }

                selected
            }
            PlacementPolicy::Custom(_labels) => {
                // For now, fall back to balanced placement
                // TODO: Implement label-based selection
                active_peers.into_iter()
                    .take(volume.replication_factor as usize)
                    .map(|p| p.id)
                    .collect()
            }
        };

        Ok(selected)
    }

    async fn broadcast_to_nodes(&self, message: &ClusterMessage, node_ids: &[Uuid]) -> Result<()> {
        let peers = self.peers.read().await;
        let addresses: Vec<SocketAddr> = node_ids.iter()
            .filter_map(|id| peers.get(id).map(|p| p.address))
            .collect();

        for addr in addresses {
            let message = message.clone();
            tokio::spawn(async move {
                if let Ok(mut stream) = TcpStream::connect(addr).await {
                    let _ = Self::send_cluster_message_static(&mut stream, &message).await;
                }
            });
        }

        Ok(())
    }

    /// Get current cluster status
    pub async fn get_status(&self) -> Result<ClusterStatus> {
        let state = self.cluster_state.read().await;
        let peers = self.peers.read().await;

        let active_nodes = peers.values()
            .filter(|p| p.status == NodeStatus::Active)
            .count() as u32;

        let (total_capacity, available_capacity) = peers.values()
            .filter(|p| p.status == NodeStatus::Active)
            .fold((0u64, 0u64), |(total, avail), node| {
                (
                    total + node.capacity.total_storage_bytes,
                    avail + node.capacity.available_storage_bytes,
                )
            });

        let zones: Vec<String> = peers.values()
            .filter_map(|p| p.zone.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        Ok(ClusterStatus {
            cluster_id: state.cluster_id,
            node_id: self.node_id,
            is_leader: state.leader_id == Some(self.node_id),
            total_nodes: peers.len() as u32,
            active_nodes,
            zones,
            total_capacity_bytes: total_capacity,
            available_capacity_bytes: available_capacity,
        })
    }

    pub async fn get_cluster_size(&self) -> Result<u32> {
        let peers = self.peers.read().await;
        Ok(peers.len() as u32)
    }

    pub async fn unregister_volume(&self, _volume_id: Uuid) -> Result<()> {
        // TODO: Implement volume unregistration
        Ok(())
    }

    /// Helper methods
    async fn detect_node_labels() -> Result<HashMap<String, String>> {
        let mut labels = HashMap::new();
        labels.insert("os".to_string(), std::env::consts::OS.to_string());
        labels.insert("arch".to_string(), std::env::consts::ARCH.to_string());
        
        if let Ok(hostname) = std::env::var("HOSTNAME") {
            labels.insert("hostname".to_string(), hostname);
        }

        Ok(labels)
    }

    async fn detect_node_capacity() -> Result<NodeCapacity> {
        // TODO: Implement actual capacity detection
        // For now, return reasonable defaults
        Ok(NodeCapacity {
            total_storage_bytes: 1_000_000_000_000, // 1TB
            available_storage_bytes: 800_000_000_000, // 800GB
            cpu_cores: 8,
            memory_bytes: 32_000_000_000, // 32GB
            network_bandwidth_mbps: 1000,
            iops_capability: 10000,
        })
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    // Network message handling
    async fn send_cluster_message(&self, stream: &mut TcpStream, message: &ClusterMessage) -> Result<()> {
        Self::send_cluster_message_static(stream, message).await
    }

    async fn send_cluster_message_static(_stream: &mut TcpStream, _message: &ClusterMessage) -> Result<()> {
        // TODO: Implement actual network protocol
        Ok(())
    }

    async fn receive_cluster_message(&self, _stream: &mut TcpStream) -> Result<ClusterMessage> {
        // TODO: Implement actual network protocol
        Ok(ClusterMessage::JoinResponse {
            accepted: true,
            cluster_state: None,
            peer_list: Vec::new(),
        })
    }
}