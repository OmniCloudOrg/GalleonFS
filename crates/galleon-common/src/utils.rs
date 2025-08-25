//! Utility functions and helpers for GalleonFS

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use byte_unit::{Byte, ByteError};
use sysinfo::{System, SystemExt};
use crate::error::{Result, GalleonError};

/// Byte size utilities
pub struct ByteSize;

impl ByteSize {
    /// Parse human-readable byte size (e.g., "1GB", "512MB", "4TB")
    pub fn parse(size_str: &str) -> Result<u64> {
        let byte = Byte::from_str(size_str)
            .map_err(|e: ByteError| GalleonError::InvalidInput(format!("Invalid byte size '{}': {}", size_str, e)))?;
        Ok(byte.get_bytes() as u64)
    }

    /// Format bytes as human-readable string
    pub fn format(bytes: u64) -> String {
        let byte = Byte::from_bytes(bytes as u128);
        let adjusted = byte.get_appropriate_unit(true);
        format!("{:.2}", adjusted)
    }

    /// Convert bytes to specific unit
    pub fn to_unit(bytes: u64, unit: &str) -> Result<f64> {
        let bytes_f64 = bytes as f64;
        match unit.to_lowercase().as_str() {
            "b" | "bytes" => Ok(bytes as f64),
            "kb" | "kilobytes" => Ok(bytes_f64 / 1024.0),
            "mb" | "megabytes" => Ok(bytes_f64 / (1024.0 * 1024.0)),
            "gb" | "gigabytes" => Ok(bytes_f64 / (1024.0 * 1024.0 * 1024.0)),
            "tb" | "terabytes" => Ok(bytes_f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0)),
            "pb" | "petabytes" => Ok(bytes_f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0)),
            _ => Err(GalleonError::InvalidInput(format!("Unknown byte unit: {}", unit))),
        }
    }
}

/// Time utilities
pub struct TimeUtils;

impl TimeUtils {
    /// Get current Unix timestamp in seconds
    pub fn unix_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs()
    }

    /// Get current Unix timestamp in milliseconds
    pub fn unix_timestamp_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis() as u64
    }

    /// Get current Unix timestamp in microseconds
    pub fn unix_timestamp_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_micros() as u64
    }

    /// Convert duration to human-readable string
    pub fn format_duration(duration: Duration) -> String {
        let secs = duration.as_secs();
        let millis = duration.subsec_millis();
        let micros = duration.subsec_micros() % 1000;

        if secs > 0 {
            format!("{}.{:03}s", secs, millis)
        } else if millis > 0 {
            format!("{}.{:03}ms", millis, micros)
        } else {
            format!("{}μs", duration.as_micros())
        }
    }

    /// Parse human-readable duration string
    pub fn parse_duration(duration_str: &str) -> Result<Duration> {
        humantime::parse_duration(duration_str)
            .map_err(|e| GalleonError::InvalidInput(format!("Invalid duration '{}': {}", duration_str, e)))
    }
}

/// System information utilities
pub struct SystemInfo;

impl SystemInfo {
    /// Get number of CPU cores
    pub fn cpu_cores() -> usize {
        num_cpus::get()
    }

    /// Get physical CPU cores
    pub fn physical_cpu_cores() -> usize {
        num_cpus::get_physical()
    }

    /// Get system page size
    pub fn page_size() -> usize {
        page_size::get()
    }

    /// Check if huge pages are available
    pub fn huge_pages_available() -> bool {
        #[cfg(target_os = "linux")]
        {
            std::path::Path::new("/sys/kernel/mm/hugepages").exists()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Get available memory in bytes
    pub fn available_memory() -> u64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
                for line in meminfo.lines() {
                    if line.starts_with("MemAvailable:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<u64>() {
                                return kb * 1024; // Convert KB to bytes
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback using sysinfo
        let mut sys = System::new();
        sys.refresh_memory();
        sys.available_memory()
    }

    /// Get total memory in bytes
    pub fn total_memory() -> u64 {
        let mut sys = System::new();
        sys.refresh_memory();
        sys.total_memory()
    }

    /// Get CPU usage percentage
    pub fn cpu_usage() -> f32 {
        let mut sys = System::new();
        sys.refresh_cpu();
        std::thread::sleep(Duration::from_millis(100)); // Wait for measurement
        sys.refresh_cpu();
        sys.global_cpu_info().cpu_usage()
    }
}

/// Network utilities
pub struct NetworkUtils;

impl NetworkUtils {
    /// Check if a port is available for binding
    pub fn is_port_available(address: &std::net::SocketAddr) -> bool {
        std::net::TcpListener::bind(address).is_ok()
    }

    /// Find next available port starting from a given port
    pub fn find_available_port(start_port: u16, max_attempts: u16) -> Option<u16> {
        for port in start_port..start_port + max_attempts {
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            if Self::is_port_available(&addr) {
                return Some(port);
            }
        }
        None
    }

    /// Get local IP addresses
    pub fn get_local_addresses() -> Vec<std::net::IpAddr> {
        let mut addresses = Vec::new();
        
        if let Ok(interfaces) = get_if_addrs::get_if_addrs() {
            for interface in interfaces {
                if !interface.is_loopback() {
                    addresses.push(interface.ip());
                }
            }
        }
        
        addresses
    }
}

/// File system utilities
pub struct FileSystemUtils;

impl FileSystemUtils {
    /// Check if a path exists and is accessible
    pub fn path_accessible(path: &std::path::Path) -> bool {
        path.exists() && path.metadata().is_ok()
    }

    /// Get available disk space for a path
    pub fn available_space(path: &std::path::Path) -> Result<u64> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| GalleonError::IoError(e))?;
        
        // This is a simplified implementation
        // In a real implementation, you'd use platform-specific APIs
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            // This is not the actual available space calculation
            // but demonstrates the pattern
            Ok(metadata.size())
        }
        
        #[cfg(not(unix))]
        {
            Ok(metadata.len())
        }
    }

    /// Create directory with parents if it doesn't exist
    pub fn ensure_dir_exists(path: &std::path::Path) -> Result<()> {
        if !path.exists() {
            std::fs::create_dir_all(path)
                .map_err(|e| GalleonError::IoError(e))?;
        }
        Ok(())
    }

    /// Get file/directory size recursively
    pub fn recursive_size(path: &std::path::Path) -> Result<u64> {
        let mut total_size = 0;
        
        if path.is_file() {
            return Ok(path.metadata()?.len());
        }
        
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                total_size += Self::recursive_size(&path)?;
            }
        }
        
        Ok(total_size)
    }
}

/// Hardware detection utilities
pub struct HardwareUtils;

impl HardwareUtils {
    /// Detect if running in a virtual machine
    pub fn is_virtual_machine() -> bool {
        #[cfg(target_os = "linux")]
        {
            // Check common VM indicators
            if let Ok(dmi) = std::fs::read_to_string("/sys/class/dmi/id/product_name") {
                let dmi = dmi.to_lowercase();
                return dmi.contains("vmware") || 
                       dmi.contains("virtualbox") || 
                       dmi.contains("kvm") || 
                       dmi.contains("qemu") ||
                       dmi.contains("hyper-v");
            }
            
            if let Ok(vendor) = std::fs::read_to_string("/sys/class/dmi/id/sys_vendor") {
                let vendor = vendor.to_lowercase();
                return vendor.contains("vmware") || 
                       vendor.contains("innotek") || 
                       vendor.contains("microsoft");
            }
        }
        
        false
    }

    /// Detect available CPU features
    pub fn cpu_features() -> CpuFeatures {
        CpuFeatures {
            #[cfg(target_arch = "x86_64")]
            aes_ni: is_x86_feature_detected!("aes"),
            #[cfg(not(target_arch = "x86_64"))]
            aes_ni: false,
            
            #[cfg(target_arch = "x86_64")]
            avx2: is_x86_feature_detected!("avx2"),
            #[cfg(not(target_arch = "x86_64"))]
            avx2: false,
            
            #[cfg(target_arch = "x86_64")]
            sha_extensions: is_x86_feature_detected!("sha"),
            #[cfg(not(target_arch = "x86_64"))]
            sha_extensions: false,
            
            #[cfg(target_arch = "x86_64")]
            avx512: is_x86_feature_detected!("avx512f"),
            #[cfg(not(target_arch = "x86_64"))]
            avx512: false,
        }
    }

    /// Get NUMA topology information
    pub fn numa_topology() -> NumaTopology {
        NumaTopology {
            node_count: Self::numa_node_count(),
            cpu_to_node: Self::cpu_to_numa_mapping(),
        }
    }

    /// Get number of NUMA nodes
    fn numa_node_count() -> u32 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(entries) = std::fs::read_dir("/sys/devices/system/node") {
                let count = entries
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| {
                        entry.file_name()
                            .to_string_lossy()
                            .starts_with("node")
                    })
                    .count();
                return count as u32;
            }
        }
        
        1 // Default to single NUMA node
    }

    /// Get CPU to NUMA node mapping
    fn cpu_to_numa_mapping() -> std::collections::HashMap<u32, u32> {
        let mut mapping = std::collections::HashMap::new();
        
        #[cfg(target_os = "linux")]
        {
            for cpu in 0..num_cpus::get() {
                let numa_path = format!("/sys/devices/system/cpu/cpu{}/numa_node", cpu);
                if let Ok(numa_str) = std::fs::read_to_string(&numa_path) {
                    if let Ok(numa_node) = numa_str.trim().parse::<u32>() {
                        mapping.insert(cpu as u32, numa_node);
                    }
                }
            }
        }
        
        // If no NUMA information available, assign all CPUs to node 0
        if mapping.is_empty() {
            for cpu in 0..num_cpus::get() {
                mapping.insert(cpu as u32, 0);
            }
        }
        
        mapping
    }
}

/// CPU feature detection results
#[derive(Debug, Clone)]
pub struct CpuFeatures {
    pub aes_ni: bool,
    pub avx2: bool,
    pub sha_extensions: bool,
    pub avx512: bool,
}

/// NUMA topology information
#[derive(Debug, Clone)]
pub struct NumaTopology {
    pub node_count: u32,
    pub cpu_to_node: std::collections::HashMap<u32, u32>,
}

/// UUID utilities
pub struct UuidUtils;

impl UuidUtils {
    /// Generate a new random UUID
    pub fn generate() -> uuid::Uuid {
        uuid::Uuid::new_v4()
    }

    /// Parse UUID from string with validation
    pub fn parse(uuid_str: &str) -> Result<uuid::Uuid> {
        uuid::Uuid::parse_str(uuid_str)
            .map_err(|e| GalleonError::InvalidInput(format!("Invalid UUID '{}': {}", uuid_str, e)))
    }

    /// Generate a short UUID (first 8 characters)
    pub fn short_uuid() -> String {
        Self::generate().to_string()[..8].to_string()
    }
}

/// Random utilities
pub struct RandomUtils;

impl RandomUtils {
    /// Generate random bytes
    pub fn random_bytes(length: usize) -> Vec<u8> {
        use ring::rand::{SecureRandom, SystemRandom};
        
        let rng = SystemRandom::new();
        let mut bytes = vec![0u8; length];
        rng.fill(&mut bytes).unwrap();
        bytes
    }

    /// Generate random string with specified charset
    pub fn random_string(length: usize, charset: &[u8]) -> String {
        let bytes = Self::random_bytes(length);
        bytes.iter()
            .map(|&b| charset[b as usize % charset.len()] as char)
            .collect()
    }

    /// Generate random alphanumeric string
    pub fn random_alphanumeric(length: usize) -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                 abcdefghijklmnopqrstuvwxyz\
                                 0123456789";
        Self::random_string(length, CHARSET)
    }
}

/// Alignment utilities for high-performance I/O
pub struct AlignmentUtils;

impl AlignmentUtils {
    /// Check if a pointer is aligned to the specified boundary
    pub fn is_aligned(ptr: *const u8, alignment: usize) -> bool {
        (ptr as usize) % alignment == 0
    }

    /// Align a size up to the next boundary
    pub fn align_up(size: usize, alignment: usize) -> usize {
        (size + alignment - 1) & !(alignment - 1)
    }

    /// Align a size down to the previous boundary
    pub fn align_down(size: usize, alignment: usize) -> usize {
        size & !(alignment - 1)
    }

    /// Create aligned buffer for O_DIRECT I/O
    pub fn create_aligned_buffer(size: usize, alignment: usize) -> Result<aligned_vec::AlignedVec<u8>> {
        let aligned_size = Self::align_up(size, alignment);
        let buffer = aligned_vec::AlignedVec::with_capacity(alignment, aligned_size);
        Ok(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_size_parse() {
        assert_eq!(ByteSize::parse("1GB").unwrap(), 1_073_741_824);
        assert_eq!(ByteSize::parse("512MB").unwrap(), 536_870_912);
        assert_eq!(ByteSize::parse("4KB").unwrap(), 4_096);
    }

    #[test]
    fn test_byte_size_format() {
        assert!(ByteSize::format(1_073_741_824).contains("GB"));
        assert!(ByteSize::format(536_870_912).contains("MB"));
        assert!(ByteSize::format(4_096).contains("KB"));
    }

    #[test]
    fn test_alignment_utils() {
        assert_eq!(AlignmentUtils::align_up(1000, 4096), 4096);
        assert_eq!(AlignmentUtils::align_down(5000, 4096), 4096);
        assert!(AlignmentUtils::is_aligned(std::ptr::null(), 4096));
    }

    #[test]
    fn test_uuid_utils() {
        let uuid = UuidUtils::generate();
        assert_eq!(uuid.to_string().len(), 36);
        
        let parsed = UuidUtils::parse(&uuid.to_string()).unwrap();
        assert_eq!(uuid, parsed);
        
        let short = UuidUtils::short_uuid();
        assert_eq!(short.len(), 8);
    }

    #[test]
    fn test_random_utils() {
        let bytes = RandomUtils::random_bytes(16);
        assert_eq!(bytes.len(), 16);
        
        let alphanumeric = RandomUtils::random_alphanumeric(10);
        assert_eq!(alphanumeric.len(), 10);
        assert!(alphanumeric.chars().all(|c| c.is_alphanumeric()));
    }
}