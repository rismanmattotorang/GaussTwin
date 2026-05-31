//! Error types for the gausstwin-data crate

use gausstwin_vec::VectorError as CrateVectorError;
use serde_json::Error as JsonError;
use std::error::Error as StdError;
use std::fmt;
use thiserror::Error;
use uuid::Error as UuidError;
use validator::ValidationErrors;

/// Comprehensive error type for data operations
#[derive(Debug)]
pub enum DataError {
    /// Errors related to data storage operations
    Storage(String),

    /// Errors related to data validation
    Validation(String),

    /// Errors related to data serialization/deserialization
    Serialization(JsonError),

    /// Errors related to connection handling
    Connection(String),

    /// Errors related to cache operations
    Cache(String),

    /// Not found errors
    NotFound { kind: ResourceKind, key: String },

    /// Rate limiting errors
    RateLimit(String),

    /// Configuration errors
    Config(String),

    /// Timeout errors
    Timeout {
        duration_secs: u64,
        operation: String,
    },

    /// Consistency errors
    Consistency(String),

    /// Internal errors
    Internal(String),

    /// Query errors
    Query(String),

    /// Pool errors
    Pool(PoolError),

    /// UUID errors
    Uuid(UuidError),

    /// Vector-related errors
    Vector(CrateVectorError),

    /// Other errors
    Other(String),
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataError::Storage(msg) => write!(f, "Storage error: {}", msg),
            DataError::Validation(msg) => write!(f, "Validation error: {}", msg),
            DataError::Connection(msg) => write!(f, "Connection error: {}", msg),
            DataError::Vector(err) => write!(f, "Vector error: {}", err),
            DataError::Pool(err) => write!(f, "Pool error: {}", err),
            DataError::Other(msg) => write!(f, "Other error: {}", msg),
            DataError::NotFound { kind, key } => {
                write!(f, "Resource not found: {} with key {}", kind, key)
            }
            DataError::RateLimit(msg) => write!(f, "Rate limit exceeded: {}", msg),
            DataError::Config(msg) => write!(f, "Config error: {}", msg),
            DataError::Timeout {
                duration_secs,
                operation,
            } => write!(f, "Operation timed out after {} seconds", duration_secs),
            DataError::Consistency(msg) => write!(f, "Consistency error: {}", msg),
            DataError::Query(msg) => write!(f, "Query error: {}", msg),
            DataError::Internal(msg) => write!(f, "Internal error: {}", msg),
            DataError::Uuid(err) => write!(f, "UUID error: {}", err),
            DataError::Serialization(err) => write!(f, "Serialization error: {}", err),
            DataError::Cache(msg) => write!(f, "Cache error: {}", msg),
        }
    }
}

impl std::error::Error for DataError {}

/// Result type alias for data operations
pub type DataResult<T> = Result<T, DataError>;

/// Storage-specific errors
#[derive(Debug)]
pub enum StorageError {
    /// Write operation failed
    WriteFailed(String),

    /// Read operation failed
    ReadFailed(String),

    /// Delete operation failed
    DeleteFailed(String),

    /// Update operation failed
    UpdateFailed(String),

    /// Initialization failed
    InitializationFailed(String),

    /// Invalid configuration
    InvalidConfig(String),

    /// Operation not supported
    NotSupported(String),

    /// Resource already exists
    AlreadyExists(String),

    /// Resource not found
    NotFound(String),

    /// Permission denied
    PermissionDenied(String),

    /// Resource busy
    ResourceBusy(String),

    /// Resource exhausted
    ResourceExhausted(String),

    /// Resource corrupted
    ResourceCorrupted(String),
}

/// Resource types for not found errors
#[derive(Debug, Clone, Copy)]
pub enum ResourceKind {
    /// Vector data
    Vector,

    /// Scalar data
    Scalar,

    /// Hybrid data
    Hybrid,

    /// Cache entry
    Cache,

    /// Configuration
    Config,

    /// Mixed data
    Mixed,

    /// Database
    Database,

    /// Pool
    Pool,
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceKind::Vector => write!(f, "Vector"),
            ResourceKind::Scalar => write!(f, "Scalar"),
            ResourceKind::Hybrid => write!(f, "Hybrid"),
            ResourceKind::Cache => write!(f, "Cache"),
            ResourceKind::Config => write!(f, "Config"),
            ResourceKind::Mixed => write!(f, "Mixed"),
            ResourceKind::Database => write!(f, "Database"),
            ResourceKind::Pool => write!(f, "Pool"),
        }
    }
}

/// Pool-specific errors
#[derive(Debug)]
pub enum PoolError {
    /// Pool initialization failed
    InitializationFailed(String),

    /// Pool is full
    PoolFull,

    /// Connection acquisition timeout
    AcquisitionTimeout(u64),

    /// Connection validation failed
    ValidationFailed(String),

    /// Connection closed
    ConnectionClosed(String),

    /// Connection error
    ConnectionError(String),

    /// Pool exhausted
    PoolExhausted,

    /// Returned connection was not valid / already closed
    InvalidConnection(String),

    /// The pool had no free connections available when requested
    NoAvailableConnections,

    /// Other pool error
    Other(String),
}

impl fmt::Display for PoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PoolError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            PoolError::PoolExhausted => write!(f, "Connection pool exhausted"),
            PoolError::InvalidConnection(msg) => write!(f, "Invalid connection: {}", msg),
            PoolError::NoAvailableConnections => write!(f, "No available connections in pool"),
            PoolError::Other(msg) => write!(f, "Other pool error: {}", msg),
            PoolError::InitializationFailed(msg) => {
                write!(f, "Pool initialization failed: {}", msg)
            }
            PoolError::AcquisitionTimeout(duration_secs) => write!(
                f,
                "Connection acquisition timeout after {} seconds",
                duration_secs
            ),
            PoolError::ValidationFailed(msg) => write!(f, "Connection validation failed: {}", msg),
            PoolError::ConnectionClosed(msg) => write!(f, "Connection closed: {}", msg),
            PoolError::PoolFull => write!(f, "Pool is full"),
        }
    }
}

impl std::error::Error for PoolError {}

impl DataError {
    /// Helper to create NotFound errors
    pub fn not_found(kind: ResourceKind, key: impl Into<String>) -> Self {
        DataError::NotFound {
            kind,
            key: key.into(),
        }
    }

    /// Helper to create timeout errors
    pub fn timeout(duration_secs: u64, operation: impl Into<String>) -> Self {
        DataError::Timeout {
            duration_secs,
            operation: operation.into(),
        }
    }
}

/// Error context trait for better error handling
pub trait ErrorContext<T> {
    fn context<C>(self, ctx: C) -> DataResult<T>
    where
        C: fmt::Display + Send + Sync + 'static;
}

impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context<C>(self, ctx: C) -> DataResult<T>
    where
        C: fmt::Display + Send + Sync + 'static,
    {
        self.map_err(|e| DataError::Internal(format!("{}: {}", ctx, e)))
    }
}

/// Implement conversion from std::io::Error
impl From<std::io::Error> for DataError {
    fn from(err: std::io::Error) -> Self {
        DataError::Internal(err.to_string())
    }
}

/// Helper macro for creating DataError::InvalidData
#[macro_export]
macro_rules! invalid_data {
    ($($arg:tt)*) => {
        $crate::error::DataError::InvalidData(format!($($arg)*))
    };
}

/// Helper macro for creating DataError::NotFound
#[macro_export]
macro_rules! not_found {
    ($($arg:tt)*) => {
        $crate::error::DataError::NotFound(format!($($arg)*))
    };
}

/// Cache error type alias
pub type CacheError = DataError;

impl From<ValidationErrors> for DataError {
    fn from(err: ValidationErrors) -> Self {
        DataError::Validation(err.to_string())
    }
}

impl From<PoolError> for DataError {
    fn from(err: PoolError) -> Self {
        match err {
            PoolError::ConnectionError(e) => DataError::Connection(e),
            other => DataError::Pool(other),
        }
    }
}

impl From<CrateVectorError> for DataError {
    fn from(err: CrateVectorError) -> Self {
        DataError::Vector(err)
    }
}

// Implement From traits
impl From<UuidError> for DataError {
    fn from(err: UuidError) -> Self {
        DataError::Uuid(err)
    }
}

impl From<JsonError> for DataError {
    fn from(err: JsonError) -> Self {
        DataError::Serialization(err)
    }
}
