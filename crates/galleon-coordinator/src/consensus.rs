//! Raft consensus implementation for GalleonFS coordinator

use galleon_common::{
    config::GalleonConfig,
    error::{Result, GalleonError},
    types::NodeId,
};
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};

/// Raft consensus engine for coordinator leader election and state replication
pub struct RaftConsensus {
    /// Node ID
    node_id: NodeId,
    /// Current term
    current_term: Arc<RwLock<u64>>,
    /// Current state (Follower, Candidate, Leader)
    state: Arc<RwLock<RaftState>>,
    /// Voted for in current term
    voted_for: Arc<RwLock<Option<NodeId>>>,
    /// Log entries
    log: Arc<RwLock<Vec<LogEntry>>>,
    /// Index of highest log entry applied to state machine
    last_applied: Arc<RwLock<u64>>,
    /// Configuration
    config: GalleonConfig,
    /// Command sender for raft operations
    command_sender: Option<mpsc::UnboundedSender<RaftCommand>>,
}

/// Raft node state
#[derive(Debug, Clone, PartialEq)]
pub enum RaftState {
    Follower,
    Candidate, 
    Leader,
}

/// Log entry for Raft consensus
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    /// Log index
    pub index: u64,
    /// Term when entry was created
    pub term: u64,
    /// Command to apply to state machine
    pub command: ConsensusCommand,
    /// Timestamp when entry was created
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Commands that can be replicated through consensus
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ConsensusCommand {
    /// No-op command (used for heartbeats)
    NoOp,
    /// Add a new node to the cluster
    AddNode { node_id: NodeId, address: std::net::SocketAddr },
    /// Remove a node from the cluster
    RemoveNode { node_id: NodeId },
    /// Update cluster configuration
    UpdateConfig { config: String },
    /// Volume metadata operation
    VolumeOperation { operation: VolumeOperation },
    /// Device registration
    RegisterDevice { device_info: String },
}

/// Volume operations that require consensus
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum VolumeOperation {
    Create { volume_id: String, metadata: String },
    Delete { volume_id: String },
    Update { volume_id: String, metadata: String },
}

/// Internal Raft commands
#[derive(Debug)]
pub enum RaftCommand {
    /// Start election timeout
    StartElection,
    /// Append entries from leader
    AppendEntries {
        term: u64,
        leader_id: NodeId,
        prev_log_index: u64,
        prev_log_term: u64,
        entries: Vec<LogEntry>,
        leader_commit: u64,
    },
    /// Request vote from candidate
    RequestVote {
        term: u64,
        candidate_id: NodeId,
        last_log_index: u64,
        last_log_term: u64,
    },
    /// Submit command to replicate
    SubmitCommand { command: ConsensusCommand },
}

impl RaftConsensus {
    /// Create a new Raft consensus instance
    pub async fn new(config: &GalleonConfig) -> Result<Self> {
        let node_id = config.node.id.unwrap_or_else(|| uuid::Uuid::new_v4());
        
        info!("Initializing Raft consensus for node {}", node_id);

        Ok(Self {
            node_id,
            current_term: Arc::new(RwLock::new(0)),
            state: Arc::new(RwLock::new(RaftState::Follower)),
            voted_for: Arc::new(RwLock::new(None)),
            log: Arc::new(RwLock::new(Vec::new())),
            last_applied: Arc::new(RwLock::new(0)),
            config: config.clone(),
            command_sender: None,
        })
    }

    /// Start the Raft consensus engine
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting Raft consensus engine");

        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        self.command_sender = Some(command_sender);

        // Start the main Raft loop
        let consensus_loop = self.clone();
        tokio::spawn(async move {
            if let Err(e) = consensus_loop.run_consensus_loop(command_receiver).await {
                error!("Raft consensus loop error: {}", e);
            }
        });

        // Start election timeout
        self.start_election_timeout().await;

        info!("Raft consensus engine started");
        Ok(())
    }

    /// Stop the Raft consensus engine
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping Raft consensus engine");
        // Implementation would gracefully stop all Raft processes
        Ok(())
    }

    /// Check if this node is the current leader
    pub async fn is_leader(&self) -> bool {
        *self.state.read() == RaftState::Leader
    }

    /// Get current term
    pub fn get_current_term(&self) -> u64 {
        *self.current_term.read()
    }

    /// Submit a command for replication
    pub async fn submit_command(&self, command: ConsensusCommand) -> Result<()> {
        if !self.is_leader().await {
            return Err(GalleonError::ConsensusError(
                "Only leader can submit commands".to_string()
            ));
        }

        if let Some(sender) = &self.command_sender {
            sender.send(RaftCommand::SubmitCommand { command })
                .map_err(|e| GalleonError::ConsensusError(format!("Failed to submit command: {}", e)))?;
        }

        Ok(())
    }

    /// Main consensus loop
    async fn run_consensus_loop(&self, mut command_receiver: mpsc::UnboundedReceiver<RaftCommand>) -> Result<()> {
        debug!("Starting Raft consensus loop");

        loop {
            tokio::select! {
                command = command_receiver.recv() => {
                    if let Some(cmd) = command {
                        self.handle_command(cmd).await?;
                    } else {
                        break; // Channel closed
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // Periodic maintenance
                    self.handle_periodic_tasks().await?;
                }
            }
        }

        Ok(())
    }

    /// Handle Raft commands
    async fn handle_command(&self, command: RaftCommand) -> Result<()> {
        match command {
            RaftCommand::StartElection => {
                self.start_election().await?;
            }
            RaftCommand::AppendEntries { term, leader_id, prev_log_index, prev_log_term, entries, leader_commit } => {
                self.handle_append_entries(term, leader_id, prev_log_index, prev_log_term, entries, leader_commit).await?;
            }
            RaftCommand::RequestVote { term, candidate_id, last_log_index, last_log_term } => {
                self.handle_request_vote(term, candidate_id, last_log_index, last_log_term).await?;
            }
            RaftCommand::SubmitCommand { command } => {
                self.handle_submit_command(command).await?;
            }
        }
        Ok(())
    }

    /// Handle periodic tasks (heartbeats, etc.)
    async fn handle_periodic_tasks(&self) -> Result<()> {
        if self.is_leader().await {
            // Send heartbeats to followers
            self.send_heartbeats().await?;
        }
        Ok(())
    }

    /// Start leader election
    async fn start_election(&self) -> Result<()> {
        info!("Starting leader election");

        // Increment current term
        {
            let mut term = self.current_term.write();
            *term += 1;
        }

        // Vote for self
        {
            let mut voted_for = self.voted_for.write();
            *voted_for = Some(self.node_id);
        }

        // Transition to candidate state
        {
            let mut state = self.state.write();
            *state = RaftState::Candidate;
        }

        // Request votes from other nodes
        self.request_votes().await?;

        Ok(())
    }

    /// Request votes from other nodes
    async fn request_votes(&self) -> Result<()> {
        // Implementation would send RequestVote RPCs to all other nodes
        // For now, simulate becoming leader if no other nodes
        if self.config.cluster.peer_addresses.is_empty() {
            info!("No peers configured, becoming leader");
            let mut state = self.state.write();
            *state = RaftState::Leader;
        }

        Ok(())
    }

    /// Handle AppendEntries RPC
    async fn handle_append_entries(
        &self,
        term: u64,
        leader_id: NodeId,
        prev_log_index: u64,
        prev_log_term: u64,
        entries: Vec<LogEntry>,
        leader_commit: u64,
    ) -> Result<()> {
        debug!("Handling AppendEntries from leader {}", leader_id);

        // If term is greater than current term, update and become follower
        if term > self.get_current_term() {
            let mut current_term = self.current_term.write();
            *current_term = term;
            let mut state = self.state.write();
            *state = RaftState::Follower;
            let mut voted_for = self.voted_for.write();
            *voted_for = None;
        }

        // Append entries to log if consistency check passes
        if self.log_consistency_check(prev_log_index, prev_log_term) {
            let mut log = self.log.write();
            
            // Remove conflicting entries
            log.truncate(prev_log_index as usize);
            
            // Append new entries
            log.extend(entries);
            
            // Update commit index
            if leader_commit > *self.last_applied.read() {
                let mut last_applied = self.last_applied.write();
                *last_applied = std::cmp::min(leader_commit, log.len() as u64);
            }
        }

        Ok(())
    }

    /// Handle RequestVote RPC
    async fn handle_request_vote(
        &self,
        term: u64,
        candidate_id: NodeId,
        last_log_index: u64,
        last_log_term: u64,
    ) -> Result<()> {
        debug!("Handling RequestVote from candidate {}", candidate_id);

        // If term is greater than current term, update
        if term > self.get_current_term() {
            let mut current_term = self.current_term.write();
            *current_term = term;
            let mut state = self.state.write();
            *state = RaftState::Follower;
            let mut voted_for = self.voted_for.write();
            *voted_for = None;
        }

        // Vote for candidate if haven't voted or already voted for this candidate
        // and candidate's log is at least as up-to-date as ours
        let voted_for = self.voted_for.read();
        if term == self.get_current_term() &&
           (voted_for.is_none() || *voted_for == Some(candidate_id)) &&
           self.is_log_up_to_date(last_log_index, last_log_term) {
            // Grant vote (implementation would send response)
            debug!("Granting vote to candidate {}", candidate_id);
        }

        Ok(())
    }

    /// Handle command submission
    async fn handle_submit_command(&self, command: ConsensusCommand) -> Result<()> {
        if !self.is_leader().await {
            return Err(GalleonError::ConsensusError(
                "Only leader can handle command submission".to_string()
            ));
        }

        let log_entry = LogEntry {
            index: {
                let log = self.log.read();
                log.len() as u64 + 1
            },
            term: self.get_current_term(),
            command,
            timestamp: chrono::Utc::now(),
        };

        // Add entry to log
        {
            let mut log = self.log.write();
            log.push(log_entry);
        }

        // Replicate to followers
        self.replicate_to_followers().await?;

        Ok(())
    }

    /// Send heartbeats to followers
    async fn send_heartbeats(&self) -> Result<()> {
        debug!("Sending heartbeats to followers");
        // Implementation would send AppendEntries RPCs with empty entries
        Ok(())
    }

    /// Replicate log entries to followers
    async fn replicate_to_followers(&self) -> Result<()> {
        debug!("Replicating log entries to followers");
        // Implementation would send AppendEntries RPCs with log entries
        Ok(())
    }

    /// Check log consistency for AppendEntries
    fn log_consistency_check(&self, prev_log_index: u64, prev_log_term: u64) -> bool {
        let log = self.log.read();
        
        if prev_log_index == 0 {
            return true;
        }

        if prev_log_index > log.len() as u64 {
            return false;
        }

        log[prev_log_index as usize - 1].term == prev_log_term
    }

    /// Check if candidate's log is at least as up-to-date as ours
    fn is_log_up_to_date(&self, last_log_index: u64, last_log_term: u64) -> bool {
        let log = self.log.read();
        
        if log.is_empty() {
            return true;
        }

        let our_last_entry = &log[log.len() - 1];
        
        // Candidate's log is more up-to-date if it has a higher term,
        // or same term with higher or equal index
        last_log_term > our_last_entry.term ||
        (last_log_term == our_last_entry.term && last_log_index >= our_last_entry.index)
    }

    /// Start election timeout
    async fn start_election_timeout(&self) {
        let consensus = self.clone();
        tokio::spawn(async move {
            // Random election timeout between 150-300ms
            let timeout_ms = 150 + (rand::random::<u64>() % 150);
            tokio::time::sleep(tokio::time::Duration::from_millis(timeout_ms)).await;
            
            // Start election if we're not a leader
            if !consensus.is_leader().await {
                if let Some(sender) = &consensus.command_sender {
                    let _ = sender.send(RaftCommand::StartElection);
                }
            }
        });
    }
}

impl Clone for RaftConsensus {
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id,
            current_term: self.current_term.clone(),
            state: self.state.clone(),
            voted_for: self.voted_for.clone(),
            log: self.log.clone(),
            last_applied: self.last_applied.clone(),
            config: self.config.clone(),
            command_sender: self.command_sender.clone(),
        }
    }
}