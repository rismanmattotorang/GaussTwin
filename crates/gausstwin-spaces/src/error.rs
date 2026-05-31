use std::fmt;
use thiserror::Error;

/// Severity levels for errors
#[derive(Debug, Clone, Copy)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Trait representing additional context information for errors
pub trait ErrorContext {}

/// Trait for providing context
pub trait ErrorContextProvider {
    fn context<C: ErrorContext + 'static>(self, ctx: C) -> Self
    where
        Self: Sized;
}

/// Trait for reporting errors (stub)
pub trait ErrorReporter {
    fn report(&self);
}

/// Error type for space operations
#[derive(Error, Debug)]
pub enum SpatialError {
    #[error("Agent {0} not found")]
    AgentNotFound(crate::AgentId),

    #[error("Position {0} is out of bounds")]
    OutOfBounds(Position),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Concurrent modification error: {0}")]
    ConcurrencyError(String),

    #[error("Index error: {0}")]
    IndexError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(String),

    #[error("Insertion into spatial index failed")]
    InsertionFailed,
}

impl From<anyhow::Error> for SpatialError {
    fn from(e: anyhow::Error) -> Self {
        SpatialError::Other(e.to_string())
    }
}

/// Position type for error reporting
#[derive(Debug)]
pub struct Position(pub String);

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Result type for space operations
pub type SpatialResult<T> = Result<T, SpatialError>;

/// Helper functions for error handling
pub(crate) mod helpers {
    use super::*;

    pub fn agent_not_found(id: crate::AgentId) -> SpatialError {
        SpatialError::AgentNotFound(id)
    }

    pub fn out_of_bounds<T: fmt::Display>(pos: T) -> SpatialError {
        SpatialError::OutOfBounds(Position(pos.to_string()))
    }

    pub fn invalid_operation<T: fmt::Display>(msg: T) -> SpatialError {
        SpatialError::InvalidOperation(msg.to_string())
    }

    pub fn concurrency_error<T: fmt::Display>(msg: T) -> SpatialError {
        SpatialError::ConcurrencyError(msg.to_string())
    }

    pub fn index_error<T: fmt::Display>(msg: T) -> SpatialError {
        SpatialError::IndexError(msg.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_conversion() {
        let id = crate::AgentId::new();
        let err = SpatialError::AgentNotFound(id);
        assert!(err.to_string().contains(&id.raw().to_string()));
    }

    #[test]
    fn test_error_helpers() {
        use helpers::*;

        let id = crate::AgentId::new();
        let err1 = agent_not_found(id);
        assert!(matches!(err1, SpatialError::AgentNotFound(_)));

        let err2 = out_of_bounds("(1.0, 2.0)");
        assert!(matches!(err2, SpatialError::OutOfBounds(_)));

        let err3 = invalid_operation("test error");
        assert!(matches!(err3, SpatialError::InvalidOperation(_)));
    }
}
