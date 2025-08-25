//! GalleonFS Storage Node
//! 
//! High-performance distributed storage node with Linux optimization
//! 
//! This crate implements the core data plane service for GalleonFS, providing:
//! - Multi-device storage management with NUMA optimization
//! - io_uring-based high-performance I/O engine
//! - Chunk-based storage with hardware-accelerated checksumming
//! - Enterprise-grade reliability and performance

pub mod storage_engine;
pub mod device_manager;
pub mod io_engine;
pub mod chunk_manager;
pub mod replication;
pub mod erasure_coding;
pub mod encryption;
pub mod compression;
pub mod block_allocator;
pub mod consistency;
pub mod striping;
pub mod tiering;
pub mod recovery;
pub mod server;

pub use storage_engine::{StorageEngine, StorageEngineStats, VolumeInfo};
pub use device_manager::{MultiDeviceManager, ClaimedDevice, DeviceType, PerformanceClass};
pub use io_engine::{IoEngine, IoRequest, IoResponse, IoOpType, IoPriority};
pub use chunk_manager::{ChunkManager, ChunkSize, ChunkMetadata, PlacementPolicy};

/// Re-export common types
pub use galleon_common::types::*;
pub use galleon_common::error::*;