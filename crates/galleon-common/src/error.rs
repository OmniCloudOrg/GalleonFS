//! Error handling for GalleonFS

use thiserror::Error;
use std::io;

/// Result type for GalleonFS operations
pub type Result<T> = std::result::Result<T, GalleonError>;

/// Result type for GalleonFS operations (convenience alias)
pub type GalleonResult<T> = std::result::Result<T, GalleonError>;

/// Main error type for GalleonFS
#[derive(Error, Debug)]
pub enum GalleonError {
    /// I/O related errors
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    /// Network communication errors
    #[error("Network error: {0}")]
    NetworkError(String),

    /// gRPC transport errors
    #[error("gRPC error: {0}")]
    GrpcError(#[from] tonic::transport::Error),

    /// gRPC status errors
    #[error("gRPC status error: {0}")]
    GrpcStatus(#[from] tonic::Status),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Cryptographic errors
    #[error("Cryptographic error: {0}")]
    CryptoError(String),

    /// Volume related errors
    #[error("Volume error: {0}")]
    VolumeError(String),

    /// Device related errors
    #[error("Device error: {0}")]
    DeviceError(String),

    /// Cluster management errors
    #[error("Cluster error: {0}")]
    ClusterError(String),

    /// Replication errors
    #[error("Replication error: {0}")]
    ReplicationError(String),

    /// Storage engine errors
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Authentication/authorization errors
    #[error("Authentication error: {0}")]
    AuthError(String),

    /// Resource not found errors
    #[error("Not found: {0}")]
    NotFound(String),

    /// Resource already exists errors
    #[error("Already exists: {0}")]
    AlreadyExists(String),

    /// Invalid input errors
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Resource exhausted errors
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    /// Timeout errors
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Consensus related errors
    #[error("Consensus error: {0}")]
    ConsensusError(String),

    /// Metadata errors
    #[error("Metadata error: {0}")]
    MetadataError(String),

    /// Backup/restore errors
    #[error("Backup error: {0}")]
    BackupError(String),

    /// Migration errors
    #[error("Migration error: {0}")]
    MigrationError(String),

    /// QoS policy errors
    #[error("QoS error: {0}")]
    QoSError(String),

    /// Hardware acceleration errors
    #[error("Hardware acceleration error: {0}")]
    HardwareError(String),

    /// Internal errors that should not normally occur
    #[error("Internal error: {0}")]
    InternalError(String),

    /// Volume not found
    #[error("Volume not found: {0}")]
    VolumeNotFound(crate::types::VolumeId),

    /// Chunk not found
    #[error("Chunk not found: {0}")]
    ChunkNotFound(crate::types::ChunkId),

    /// Invalid chunk size
    #[error("Invalid chunk size: {0}")]
    InvalidChunkSize(usize),

    /// Insufficient capacity
    #[error("Insufficient capacity: required {required}, available {available}")]
    InsufficientCapacity { required: u64, available: u64 },

    /// Insufficient resources
    #[error("Insufficient {resource}: required {required}, available {available}")]
    InsufficientResources { resource: String, required: usize, available: usize },

    /// Allocation too large
    #[error("Allocation too large: {0}")]
    AllocationTooLarge(usize),

    /// Allocation failed
    #[error("Allocation failed: {0}")]
    AllocationFailed(String),

    /// Allocation not found
    #[error("Allocation not found at offset: {0}")]
    AllocationNotFound(u64),

    /// System error
    #[error("System error: {0}")]
    SystemError(String),
}

impl From<serde_json::Error> for GalleonError {
    fn from(err: serde_json::Error) -> Self {
        GalleonError::SerializationError(format!("JSON error: {}", err))
    }
}

impl From<bincode::Error> for GalleonError {
    fn from(err: bincode::Error) -> Self {
        GalleonError::SerializationError(format!("Bincode error: {}", err))
    }
}

impl From<uuid::Error> for GalleonError {
    fn from(err: uuid::Error) -> Self {
        GalleonError::InvalidInput(format!("UUID error: {}", err))
    }
}

impl From<std::num::ParseIntError> for GalleonError {
    fn from(err: std::num::ParseIntError) -> Self {
        GalleonError::InvalidInput(format!("Parse error: {}", err))
    }
}

impl From<std::str::Utf8Error> for GalleonError {
    fn from(err: std::str::Utf8Error) -> Self {
        GalleonError::InvalidInput(format!("UTF-8 error: {}", err))
    }
}

impl From<chrono::ParseError> for GalleonError {
    fn from(err: chrono::ParseError) -> Self {
        GalleonError::InvalidInput(format!("Date parse error: {}", err))
    }
}

/// Convert GalleonError to gRPC Status for network transmission
impl From<GalleonError> for tonic::Status {
    fn from(err: GalleonError) -> Self {
        use tonic::Code;
        
        match err {
            GalleonError::NotFound(_) => tonic::Status::new(Code::NotFound, err.to_string()),
            GalleonError::AlreadyExists(_) => tonic::Status::new(Code::AlreadyExists, err.to_string()),
            GalleonError::InvalidInput(_) => tonic::Status::new(Code::InvalidArgument, err.to_string()),
            GalleonError::ResourceExhausted(_) => tonic::Status::new(Code::ResourceExhausted, err.to_string()),
            GalleonError::Timeout(_) => tonic::Status::new(Code::DeadlineExceeded, err.to_string()),
            GalleonError::AuthError(_) => tonic::Status::new(Code::Unauthenticated, err.to_string()),
            GalleonError::NetworkError(_) => tonic::Status::new(Code::Unavailable, err.to_string()),
            GalleonError::InternalError(_) => tonic::Status::new(Code::Internal, err.to_string()),
            _ => tonic::Status::new(Code::Unknown, err.to_string()),
        }
    }
}

/// Error context trait for adding additional information to errors
pub trait ErrorContext<T> {
    /// Add context to an error
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T, E> ErrorContext<T> for std::result::Result<T, E>
where
    E: Into<GalleonError>,
{
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| {
            let base_error = e.into();
            let context = f();
            match base_error {
                GalleonError::IoError(io_err) => {
                    GalleonError::IoError(io::Error::new(io_err.kind(), format!("{}: {}", context, io_err)))
                }
                other => {
                    GalleonError::InternalError(format!("{}: {}", context, other))
                }
            }
        })
    }
}

/// Macro for creating context-aware errors
#[macro_export]
macro_rules! galleon_error {
    ($kind:ident, $($arg:tt)*) => {
        $crate::error::GalleonError::$kind(format!($($arg)*))
    };
}

/// Macro for returning early with an error and context
#[macro_export]
macro_rules! bail {
    ($kind:ident, $($arg:tt)*) => {
        return Err($crate::galleon_error!($kind, $($arg)*))
    };
}

/// Macro for ensuring a condition or returning an error
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $kind:ident, $($arg:tt)*) => {
        if !($cond) {
            $crate::bail!($kind, $($arg)*);
        }
    };
}