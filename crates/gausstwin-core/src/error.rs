//! Error types for GaussTwin
//!
//! This module provides comprehensive error handling for all GaussTwin operations.

use crate::agent::AgentId;
use thiserror::Error;

/// Result type alias for GaussTwin operations
pub type Result<T> = std::result::Result<T, GaussTwinError>;

/// Comprehensive error types for GaussTwin operations
#[derive(Error, Debug, Clone)]
pub enum GaussTwinError {
    /// Agent-related errors
    #[error("Agent with ID {0:?} not found")]
    AgentNotFound(AgentId),
    /// Agent type not registered
    #[error("Agent type '{0}' not registered")]
    AgentTypeNotFound(String),
    /// Invalid agent state
    #[error("Invalid agent state: {0}")]
    InvalidAgentState(String),
    /// Spatial errors
    #[error("Dimension mismatch: expected {0}, got {1}")]
    DimensionMismatch(usize, usize),
    /// Index out of bounds for a given dimension
    #[error("Index {0} out of bounds for dimension {1}")]
    IndexOutOfBounds(usize, usize),
    /// Invalid position string
    #[error("Invalid position: {0}")]
    InvalidPosition(String),
    /// Position is out of bounds
    #[error("Position out of bounds")]
    OutOfBounds,
    /// Invalid time step value
    #[error("Invalid time step: {0}")]
    InvalidTimeStep(f64),
    /// Scheduler error
    #[error("Scheduler error: {0}")]
    SchedulerError(String),
    /// Event queue overflow
    #[error("Event queue overflow")]
    EventQueueOverflow,
    /// Model not initialized
    #[error("Model not initialized")]
    ModelNotInitialized,
    /// Invalid model configuration
    #[error("Invalid model configuration: {0}")]
    InvalidModelConfig(String),
    /// Model execution failed
    #[error("Model execution failed: {0}")]
    ModelExecutionFailed(String),
    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(String),
    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),
    /// Deserialization error
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),
    /// API error
    #[error("API error: {0}")]
    ApiError(String),
    /// Authentication failed
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    /// Authorization failed
    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),
    /// vLLM error
    #[error("vLLM error: {0}")]
    VllmError(String),
    /// Milvus error
    #[error("Milvus error: {0}")]
    MilvusError(String),
    /// ML model error
    #[error("ML model error: {0}")]
    MlModelError(String),
    /// MARL error
    #[error("MARL error: {0}")]
    MarlError(String),
    /// Resource exhausted error
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
    /// Timeout occurred
    #[error("Timeout occurred: {0}")]
    Timeout(String),
    /// Out of memory error
    #[error("Memory allocation failed")]
    OutOfMemory,
    /// Thread pool error
    #[error("Thread pool error: {0}")]
    ThreadPoolError(String),
    /// Validation failed
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    /// Constraint violation
    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),
    /// Invariant violation
    #[error("Invariant violation: {0}")]
    InvariantViolation(String),
    /// I/O error
    #[error("I/O error: {0}")]
    IoError(String),
    /// File not found error
    #[error("File not found: {0}")]
    FileNotFound(String),
    /// Permission denied error
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),
    /// Missing required parameter
    #[error("Missing required parameter: {0}")]
    MissingParameter(String),
    /// Invalid parameter value
    #[error("Invalid parameter value: {0}")]
    InvalidParameter(String),
    /// External service unavailable
    #[error("External service unavailable: {0}")]
    ServiceUnavailable(String),
    /// External service error
    #[error("External service error: {0}")]
    ExternalServiceError(String),
    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
    /// Not implemented error
    #[error("Not implemented: {0}")]
    NotImplemented(String),
    /// Operation not supported error
    #[error("Operation not supported: {0}")]
    NotSupported(String),
    /// Unknown error
    #[error("Unknown error: {0}")]
    Unknown(String),
    /// Custom error message
    #[error("Custom error: {0}")]
    Custom(String),
    /// Capacity exceeded error
    #[error("Capacity exceeded: {0}")]
    CapacityExceeded(String),
}

impl GaussTwinError {
    /// Check if this is a recoverable error
    pub fn is_recoverable(&self) -> bool {
        match self {
            // Temporary failures that might succeed on retry
            GaussTwinError::NetworkError(_)
            | GaussTwinError::Timeout(_)
            | GaussTwinError::ServiceUnavailable(_)
            | GaussTwinError::ResourceExhausted(_)
            | GaussTwinError::ThreadPoolError(_) => true,

            // Permanent failures that won't succeed on retry
            GaussTwinError::AgentNotFound(_)
            | GaussTwinError::AgentTypeNotFound(_)
            | GaussTwinError::ValidationFailed(_)
            | GaussTwinError::ConstraintViolation(_)
            | GaussTwinError::InvalidParameter(_)
            | GaussTwinError::FileNotFound(_)
            | GaussTwinError::PermissionDenied(_)
            | GaussTwinError::NotImplemented(_)
            | GaussTwinError::NotSupported(_) => false,

            // Other errors might be recoverable depending on context
            _ => false,
        }
    }

    /// Get error category for telemetry and monitoring
    pub fn category(&self) -> ErrorCategory {
        match self {
            GaussTwinError::AgentNotFound(_)
            | GaussTwinError::AgentTypeNotFound(_)
            | GaussTwinError::InvalidAgentState(_) => ErrorCategory::Agent,

            GaussTwinError::DimensionMismatch(_, _)
            | GaussTwinError::IndexOutOfBounds(_, _)
            | GaussTwinError::InvalidPosition(_)
            | GaussTwinError::OutOfBounds => ErrorCategory::Spatial,

            GaussTwinError::InvalidTimeStep(_)
            | GaussTwinError::SchedulerError(_)
            | GaussTwinError::EventQueueOverflow => ErrorCategory::Time,

            GaussTwinError::ModelNotInitialized
            | GaussTwinError::InvalidModelConfig(_)
            | GaussTwinError::ModelExecutionFailed(_) => ErrorCategory::Model,

            GaussTwinError::DatabaseError(_)
            | GaussTwinError::SerializationError(_)
            | GaussTwinError::DeserializationError(_) => ErrorCategory::Persistence,

            GaussTwinError::NetworkError(_)
            | GaussTwinError::ApiError(_)
            | GaussTwinError::AuthenticationFailed(_)
            | GaussTwinError::AuthorizationFailed(_) => ErrorCategory::Network,

            GaussTwinError::VllmError(_)
            | GaussTwinError::MilvusError(_)
            | GaussTwinError::MlModelError(_)
            | GaussTwinError::MarlError(_) => ErrorCategory::AiMl,

            GaussTwinError::ResourceExhausted(_)
            | GaussTwinError::Timeout(_)
            | GaussTwinError::OutOfMemory
            | GaussTwinError::ThreadPoolError(_) => ErrorCategory::Resource,

            GaussTwinError::ValidationFailed(_)
            | GaussTwinError::ConstraintViolation(_)
            | GaussTwinError::InvariantViolation(_) => ErrorCategory::Validation,

            GaussTwinError::IoError(_)
            | GaussTwinError::FileNotFound(_)
            | GaussTwinError::PermissionDenied(_) => ErrorCategory::Io,

            GaussTwinError::ConfigError(_)
            | GaussTwinError::MissingParameter(_)
            | GaussTwinError::InvalidParameter(_) => ErrorCategory::Configuration,

            GaussTwinError::ServiceUnavailable(_) | GaussTwinError::ExternalServiceError(_) => {
                ErrorCategory::External
            }

            GaussTwinError::Internal(_)
            | GaussTwinError::NotImplemented(_)
            | GaussTwinError::NotSupported(_)
            | GaussTwinError::Unknown(_) => ErrorCategory::System,
            GaussTwinError::Custom(_) => ErrorCategory::System,
            GaussTwinError::CapacityExceeded(_) => ErrorCategory::Resource,
        }
    }

    /// Get error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            // Critical errors that indicate serious system problems
            GaussTwinError::OutOfMemory
            | GaussTwinError::InvariantViolation(_)
            | GaussTwinError::Internal(_) => ErrorSeverity::Critical,

            // High severity errors that prevent normal operation
            GaussTwinError::ModelNotInitialized
            | GaussTwinError::DatabaseError(_)
            | GaussTwinError::AuthenticationFailed(_)
            | GaussTwinError::PermissionDenied(_) => ErrorSeverity::High,

            // Medium severity errors that affect functionality
            GaussTwinError::AgentNotFound(_)
            | GaussTwinError::ValidationFailed(_)
            | GaussTwinError::NetworkError(_)
            | GaussTwinError::ServiceUnavailable(_) => ErrorSeverity::Medium,

            // Low severity errors that are expected or easily recoverable
            GaussTwinError::Timeout(_)
            | GaussTwinError::NotSupported(_)
            | GaussTwinError::InvalidParameter(_) => ErrorSeverity::Low,

            // Default to medium for other errors
            _ => ErrorSeverity::Medium,
        }
    }
}

/// Error categories for telemetry and monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// Agent-related errors
    Agent,
    /// Spatial computation errors
    Spatial,
    /// Time and scheduling errors
    Time,
    /// Model execution errors
    Model,
    /// Data persistence errors
    Persistence,
    /// Network and API errors
    Network,
    /// AI/ML integration errors
    AiMl,
    /// Resource and performance errors
    Resource,
    /// Validation errors
    Validation,
    /// I/O errors
    Io,
    /// Configuration errors
    Configuration,
    /// External service errors
    External,
    /// System errors
    System,
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Low severity - expected or easily recoverable
    Low,
    /// Medium severity - affects functionality
    Medium,
    /// High severity - prevents normal operation
    High,
    /// Critical severity - indicates serious system problems
    Critical,
}

// Standard library error conversions
impl From<std::io::Error> for GaussTwinError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => GaussTwinError::FileNotFound(err.to_string()),
            std::io::ErrorKind::PermissionDenied => {
                GaussTwinError::PermissionDenied(err.to_string())
            }
            std::io::ErrorKind::TimedOut => GaussTwinError::Timeout(err.to_string()),
            _ => GaussTwinError::IoError(err.to_string()),
        }
    }
}

impl From<serde_json::Error> for GaussTwinError {
    fn from(err: serde_json::Error) -> Self {
        if err.is_syntax() || err.is_data() {
            GaussTwinError::DeserializationError(err.to_string())
        } else {
            GaussTwinError::SerializationError(err.to_string())
        }
    }
}

impl From<uuid::Error> for GaussTwinError {
    fn from(err: uuid::Error) -> Self {
        GaussTwinError::ValidationFailed(format!("Invalid UUID: {}", err))
    }
}

#[cfg(feature = "parallel")]
impl From<rayon::ThreadPoolBuildError> for GaussTwinError {
    fn from(err: rayon::ThreadPoolBuildError) -> Self {
        GaussTwinError::ThreadPoolError(err.to_string())
    }
}

// Async runtime errors
impl From<tokio::time::error::Elapsed> for GaussTwinError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        GaussTwinError::Timeout("Operation timed out".to_string())
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for GaussTwinError {
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        GaussTwinError::ThreadPoolError(format!("Channel send error: {}", err))
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for GaussTwinError {
    fn from(err: tokio::sync::oneshot::error::RecvError) -> Self {
        GaussTwinError::ThreadPoolError(format!("Channel receive error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_categorization() {
        let agent_error = GaussTwinError::AgentNotFound(crate::agent::AgentId::new());
        assert_eq!(agent_error.category(), ErrorCategory::Agent);
        assert_eq!(agent_error.severity(), ErrorSeverity::Medium);
        assert!(!agent_error.is_recoverable());
    }

    #[test]
    fn test_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let gauss_error = GaussTwinError::from(io_error);

        match gauss_error {
            GaussTwinError::FileNotFound(_) => {}
            _ => panic!("Expected FileNotFound error"),
        }
    }

    #[test]
    fn test_timeout_error() {
        let timeout_error = GaussTwinError::Timeout("Operation timed out".to_string());
        assert!(timeout_error.is_recoverable());
        assert_eq!(timeout_error.severity(), ErrorSeverity::Low);
    }
}
