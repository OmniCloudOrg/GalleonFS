//! # GalleonFS Common
//! 
//! Shared libraries and types for the GalleonFS enterprise-grade distributed storage system.
//! This crate provides the foundational components used across all GalleonFS services.

pub mod types;
pub mod network;
pub mod crypto;
pub mod metrics;
pub mod error;
pub mod config;
pub mod volume_topology;
pub mod utils;

// Re-export commonly used types
pub use types::*;
pub use error::{GalleonError, Result};
pub use network::NetworkProtocol;
pub use crypto::*;
pub use volume_topology::*;

/// Current version of the GalleonFS protocol
pub const GALLEON_PROTOCOL_VERSION: u32 = 1;

/// Default block size for storage operations (4KB)
pub const DEFAULT_BLOCK_SIZE: u64 = 4096;

/// Maximum block size supported (16MB)
pub const MAX_BLOCK_SIZE: u64 = 16 * 1024 * 1024;

/// Default chunk size for large volume management (64MB)
pub const DEFAULT_CHUNK_SIZE: u64 = 64 * 1024 * 1024;

/// Maximum chunk size supported (1GB)
pub const MAX_CHUNK_SIZE: u64 = 1024 * 1024 * 1024;

/// Maximum number of devices per storage node
pub const MAX_DEVICES_PER_NODE: u32 = 1000;

/// Maximum number of nodes in a cluster
pub const MAX_CLUSTER_NODES: u32 = 10000;

/// Default replication factor
pub const DEFAULT_REPLICATION_FACTOR: u8 = 3;

/// Maximum replication factor
pub const MAX_REPLICATION_FACTOR: u8 = 16;