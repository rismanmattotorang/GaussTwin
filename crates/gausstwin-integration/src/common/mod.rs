//! Common utilities for integration layer

use openssl::error::ErrorStack;
use openssl::ssl::{SslConnector, SslMethod};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Integration error types
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Timeout error: {0}")]
    Timeout(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Unsupported connector: {0}")]
    UnsupportedConnector(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for integration operations
pub type Result<T> = std::result::Result<T, Error>;

/// Integration metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    /// Number of successful connections
    pub connections: u64,

    /// Number of connection failures
    pub connection_failures: u64,

    /// Number of messages sent
    pub messages_sent: u64,

    /// Number of messages received
    pub messages_received: u64,

    /// Number of errors
    pub errors: u64,

    /// Average latency in milliseconds
    pub average_latency_ms: f64,

    /// Bytes sent
    pub bytes_sent: u64,

    /// Bytes received
    pub bytes_received: u64,

    /// Connection uptime in seconds
    pub uptime_seconds: u64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            connections: 0,
            connection_failures: 0,
            messages_sent: 0,
            messages_received: 0,
            errors: 0,
            average_latency_ms: 0.0,
            bytes_sent: 0,
            bytes_received: 0,
            uptime_seconds: 0,
        }
    }
}

/// Retry utilities
pub mod retry {
    use super::*;
    use tokio::time::{sleep, Duration};

    pub async fn with_backoff<T, F, Fut>(mut f: F, retry_policy: &crate::RetryPolicy) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut retries = 0;
        let mut backoff = retry_policy.initial_backoff;

        loop {
            match f().await {
                Ok(value) => return Ok(value),
                Err(e) => {
                    if retries >= retry_policy.max_retries {
                        return Err(e);
                    }

                    sleep(backoff).await;

                    retries += 1;
                    backoff = std::cmp::min(
                        backoff.mul_f64(retry_policy.backoff_multiplier),
                        retry_policy.max_backoff,
                    );
                }
            }
        }
    }
}

/// Security utilities
pub mod security {
    use super::*;
    use openssl::ssl::{SslConnector, SslMethod};

    pub fn create_ssl_connector(
        certificate_path: &str,
        private_key_path: &str,
    ) -> Result<SslConnector> {
        let mut builder = SslConnector::builder(SslMethod::tls())?;
        builder.set_certificate_file(certificate_path, openssl::ssl::SslFiletype::PEM)?;
        builder.set_private_key_file(private_key_path, openssl::ssl::SslFiletype::PEM)?;
        Ok(builder.build())
    }
}

/// Validation utilities
pub mod validation {
    use super::*;

    pub fn validate_config(config: &crate::Config) -> Result<()> {
        if config.name.is_empty() {
            return Err(Error::Configuration("name cannot be empty".into()));
        }

        if config.connector_type.is_empty() {
            return Err(Error::Configuration(
                "connector_type cannot be empty".into(),
            ));
        }

        if config.timeout.as_secs() == 0 {
            return Err(Error::Configuration("timeout cannot be zero".into()));
        }

        Ok(())
    }
}

impl From<ErrorStack> for Error {
    fn from(err: ErrorStack) -> Self {
        Error::Connection(format!("OpenSSL error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_default() {
        let metrics = Metrics::default();
        assert_eq!(metrics.connections, 0);
        assert_eq!(metrics.errors, 0);
        assert_eq!(metrics.average_latency_ms, 0.0);
    }

    #[tokio::test]
    async fn test_retry_with_backoff() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let retry_policy = crate::RetryPolicy {
            max_retries: 3,
            initial_backoff: std::time::Duration::from_millis(10),
            max_backoff: std::time::Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = retry::with_backoff(
            move || {
                let attempts = attempts_clone.clone();
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                    if count < 3 {
                        Err(Error::Connection("test error".into()))
                    } else {
                        Ok(())
                    }
                }
            },
            &retry_policy,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
