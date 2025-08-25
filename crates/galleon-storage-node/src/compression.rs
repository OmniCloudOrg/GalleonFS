//! Compression module for data compression
//! 
//! This module provides:
//! - Multiple compression algorithms (LZ4, Zstd, Snappy)
//! - Hardware-accelerated compression where available
//! - Adaptive compression based on data patterns

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Compression algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    None,
    Lz4,
    Zstd,
    Snappy,
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    pub algorithm: CompressionAlgorithm,
    pub compression_level: u32,
    pub enable_adaptive: bool,
}

/// Compression manager
pub struct CompressionManager {
    config: CompressionConfig,
}

impl CompressionManager {
    pub fn new(config: CompressionConfig) -> Self {
        Self { config }
    }
    
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self.config.algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Lz4 => {
                Ok(lz4::block::compress(data, None, false)?)
            }
            CompressionAlgorithm::Zstd => {
                Ok(zstd::encode_all(data, self.config.compression_level as i32)?)
            }
            CompressionAlgorithm::Snappy => {
                let mut encoder = snap::raw::Encoder::new();
                Ok(encoder.compress_vec(data)?)
            }
        }
    }
    
    pub fn decompress(&self, data: &[u8], original_size: usize) -> Result<Vec<u8>> {
        match self.config.algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Lz4 => {
                Ok(lz4::block::decompress(data, Some(original_size as i32))?)
            }
            CompressionAlgorithm::Zstd => {
                Ok(zstd::decode_all(data)?)
            }
            CompressionAlgorithm::Snappy => {
                let mut decoder = snap::raw::Decoder::new();
                Ok(decoder.decompress_vec(data)?)
            }
        }
    }
    
    pub fn get_compression_ratio(&self, original_size: usize, compressed_size: usize) -> f64 {
        if original_size == 0 {
            1.0
        } else {
            original_size as f64 / compressed_size as f64
        }
    }
}