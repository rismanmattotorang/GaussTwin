use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Result type for the API server
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for the API server
#[derive(Error, Debug)]
pub enum Error {
    /// Database error
    #[error("Database error: {0}")]
    Database(String),

    /// Cache error
    #[error("Cache error: {0}")]
    Cache(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    Authentication(String),

    /// Authorization error
    #[error("Authorization error: {0}")]
    Authorization(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Invalid token
    #[error("Invalid token: {0}")]
    InvalidToken(String),

    /// Token expired
    #[error("Token expired")]
    TokenExpired,

    /// Invalid credentials
    #[error("Invalid credentials")]
    InvalidCredentials,

    /// Invalid request
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Conflict
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    /// Internal server error
    #[error("Internal server error: {0}")]
    Internal(String),

    /// External service error
    #[error("External service error: {0}")]
    ExternalService(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// JWT error
    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    /// SurrealDB error
    #[error("SurrealDB error: {0}")]
    SurrealDB(String),

    /// Milvus error
    #[error("Milvus error: {0}")]
    Milvus(String),

    /// SkyTable error
    #[error("SkyTable error: {0}")]
    SkyTable(String),

    /// Password hashing error
    #[error("Password hashing error: {0}")]
    PasswordHashing(String),

    /// GraphQL error
    #[error("GraphQL error: {0}")]
    GraphQL(String),

    /// gRPC error
    #[error("gRPC error: {0}")]
    Grpc(String),

    /// WebSocket error
    #[error("WebSocket error: {0}")]
    WebSocket(String),
}

/// Error response for the API
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Error details
    pub details: Option<serde_json::Value>,
}

impl Error {
    /// Convert the error to an HTTP status code
    pub fn status_code(&self) -> u16 {
        match self {
            Error::Authentication(_) => 401,
            Error::Authorization(_) => 403,
            Error::PermissionDenied(_) => 403,
            Error::InvalidToken(_) => 401,
            Error::TokenExpired => 401,
            Error::InvalidCredentials => 401,
            Error::InvalidRequest(_) => 400,
            Error::InvalidInput(_) => 400,
            Error::NotFound(_) => 404,
            Error::Conflict(_) => 409,
            Error::RateLimitExceeded => 429,
            Error::Validation(_) => 422,
            _ => 500,
        }
    }

    /// Convert the error to an error code string
    pub fn error_code(&self) -> String {
        match self {
            Error::Authentication(_) => "AUTHENTICATION_ERROR",
            Error::Authorization(_) => "AUTHORIZATION_ERROR",
            Error::PermissionDenied(_) => "PERMISSION_DENIED",
            Error::InvalidToken(_) => "INVALID_TOKEN",
            Error::TokenExpired => "TOKEN_EXPIRED",
            Error::InvalidCredentials => "INVALID_CREDENTIALS",
            Error::InvalidRequest(_) => "INVALID_REQUEST",
            Error::InvalidInput(_) => "INVALID_INPUT",
            Error::NotFound(_) => "NOT_FOUND",
            Error::Conflict(_) => "CONFLICT",
            Error::RateLimitExceeded => "RATE_LIMIT_EXCEEDED",
            Error::Internal(_) => "INTERNAL_ERROR",
            Error::ExternalService(_) => "EXTERNAL_SERVICE_ERROR",
            Error::Validation(_) => "VALIDATION_ERROR",
            Error::Configuration(_) => "CONFIGURATION_ERROR",
            Error::Database(_) => "DATABASE_ERROR",
            Error::Cache(_) => "CACHE_ERROR",
            Error::SurrealDB(_) => "SURREALDB_ERROR",
            Error::Milvus(_) => "MILVUS_ERROR",
            Error::SkyTable(_) => "SKYTABLE_ERROR",
            Error::PasswordHashing(_) => "PASSWORD_HASHING_ERROR",
            Error::GraphQL(_) => "GRAPHQL_ERROR",
            Error::Grpc(_) => "GRPC_ERROR",
            Error::WebSocket(_) => "WEBSOCKET_ERROR",
            Error::Io(_) => "IO_ERROR",
            Error::Json(_) => "JSON_ERROR",
            Error::Jwt(_) => "JWT_ERROR",
        }
        .to_string()
    }

    /// Convert the error to an error response
    pub fn to_response(&self) -> ErrorResponse {
        ErrorResponse {
            code: self.error_code(),
            message: self.to_string(),
            details: None,
        }
    }
}

/// Implement From traits for common error types
impl From<String> for Error {
    fn from(err: String) -> Self {
        Error::Internal(err)
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Error::Internal(err.to_string())
    }
}

impl From<argon2::password_hash::Error> for Error {
    fn from(err: argon2::password_hash::Error) -> Self {
        Error::PasswordHashing(err.to_string())
    }
}

impl From<skytable::error::Error> for Error {
    fn from(err: skytable::error::Error) -> Self {
        Error::SkyTable(err.to_string())
    }
}

impl From<surrealdb::Error> for Error {
    fn from(err: surrealdb::Error) -> Self {
        Error::SurrealDB(err.to_string())
    }
}

// Milvus error conversion removed - dependency not available in this crate

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_conversion() {
        let err = Error::Authentication("Invalid token".into());
        assert_eq!(err.status_code(), 401);
        assert_eq!(err.error_code(), "AUTHENTICATION_ERROR");

        let response = err.to_response();
        assert_eq!(response.code, "AUTHENTICATION_ERROR");
        assert_eq!(response.message, "Authentication error: Invalid token");
    }
}
