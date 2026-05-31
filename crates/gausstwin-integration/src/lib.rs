//! GaussTwin Integration Layer
//!
//! Provides comprehensive integration capabilities for GaussTwin Enterprise platform.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};

pub mod blockchain;
pub mod cloud;
pub mod common;
pub mod connectors;
pub mod industrial;
pub mod io;

// Re-exports
pub use common::{Error, Result};

/// Core trait for all integration connectors
#[async_trait]
pub trait Connector: Send + Sync {
    /// Initialize the connector
    async fn connect(&mut self) -> Result<()>;

    /// Disconnect and cleanup
    async fn disconnect(&mut self) -> Result<()>;

    /// Check connection status
    async fn is_connected(&self) -> bool;

    /// Get connector metrics
    fn metrics(&self) -> &common::Metrics;
}

/// Data format supported by the integration layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataFormat {
    Json,
    Arrow,
    Parquet,
    Avro,
    Protobuf,
    Raw(Vec<u8>),
}

/// Integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub name: String,
    pub connector_type: String,
    pub auth: AuthConfig,
    pub retry_policy: RetryPolicy,
    pub timeout: std::time::Duration,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub auth_type: AuthType,
    pub credentials: Credentials,
}

/// Authentication types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthType {
    None,
    Basic,
    OAuth2,
    Token,
    Certificate,
    Custom(String),
}

/// Credentials for authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub username: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
    pub certificate_path: Option<String>,
    pub private_key_path: Option<String>,
    pub custom: Option<serde_json::Value>,
}

impl Default for Credentials {
    fn default() -> Self {
        Self {
            username: None,
            password: None,
            token: None,
            certificate_path: None,
            private_key_path: None,
            custom: None,
        }
    }
}

/// Retry policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_backoff: std::time::Duration,
    pub max_backoff: std::time::Duration,
    pub backoff_multiplier: f64,
}

/// Integration events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Connected {
        connector: String,
    },
    Disconnected {
        connector: String,
    },
    DataReceived {
        connector: String,
        format: DataFormat,
    },
    DataSent {
        connector: String,
        format: DataFormat,
    },
    Error {
        connector: String,
        error: String,
    },
}

/// Integration status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub connector: String,
    pub state: State,
    pub last_error: Option<String>,
    pub connected_since: Option<chrono::DateTime<chrono::Utc>>,
    pub metrics: common::Metrics,
}

/// Connector state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Error,
}

/// Integration builder
pub struct IntegrationBuilder {
    config: Config,
}

impl IntegrationBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            config: Config {
                name: name.into(),
                connector_type: String::new(),
                auth: AuthConfig {
                    auth_type: AuthType::None,
                    credentials: Credentials {
                        username: None,
                        password: None,
                        token: None,
                        certificate_path: None,
                        private_key_path: None,
                        custom: None,
                    },
                },
                retry_policy: RetryPolicy {
                    max_retries: 3,
                    initial_backoff: std::time::Duration::from_secs(1),
                    max_backoff: std::time::Duration::from_secs(60),
                    backoff_multiplier: 2.0,
                },
                timeout: std::time::Duration::from_secs(30),
            },
        }
    }

    pub fn connector_type(mut self, connector_type: impl Into<String>) -> Self {
        self.config.connector_type = connector_type.into();
        self
    }

    pub fn auth(mut self, auth: AuthConfig) -> Self {
        self.config.auth = auth;
        self
    }

    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.config.retry_policy = retry_policy;
        self
    }

    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    pub async fn build(self) -> Result<Box<dyn Connector>> {
        // Factory pattern to create appropriate connector
        match self.config.connector_type.as_str() {
            // IoT & Edge
            "mqtt" => Ok(Box::new(
                connectors::mqtt::MqttConnector::new(self.config).await?,
            )),
            "opcua" => Ok(Box::new(
                connectors::opcua::OpcUaConnector::new(self.config).await?,
            )),
            "modbus" => Ok(Box::new(
                connectors::modbus::ModbusConnector::new(self.config).await?,
            )),

            // Cloud
            "aws" => Ok(Box::new(cloud::aws::AWSConnector::new(
                cloud::aws::AWSConfig::from(self.config),
            ))),
            "azure" => Ok(Box::new(cloud::azure::AzureConnector::new(
                cloud::azure::AzureConfig::from(self.config),
            ))),
            "gcp" => Ok(Box::new(cloud::gcp::GCPConnector::new(
                cloud::gcp::GCPConfig::from(self.config),
            ))),

            // Blockchain
            "ethereum" => Ok(Box::new(blockchain::ethereum::EthereumConnector::new(
                blockchain::ethereum::EthereumConfig::from(self.config),
            ))),
            // "solana" => Ok(Box::new(blockchain::solana::SolanaConnector::new(self.config).await?)),

            // Message Brokers
            "kafka" => Ok(Box::new(
                connectors::kafka::KafkaConnector::new(self.config).await?,
            )),
            "rabbitmq" => Ok(Box::new(
                connectors::rabbitmq::RabbitMqConnector::new(self.config).await?,
            )),

            // Databases
            "mongodb" => Ok(Box::new(
                connectors::mongodb::MongoDbConnector::new(self.config).await?,
            )),
            "postgresql" => Ok(Box::new(
                connectors::postgresql::PostgresConnector::new(self.config).await?,
            )),

            // Industrial
            "s7" => Ok(Box::new(
                industrial::s7::S7Connector::new(self.config).await?,
            )),
            "bacnet" => Ok(Box::new(
                industrial::bacnet::BacnetConnector::new(self.config).await?,
            )),

            _ => Err(Error::UnsupportedConnector(self.config.connector_type)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test::block_on;

    #[test]
    fn test_builder() {
        let builder = IntegrationBuilder::new("test")
            .connector_type("mqtt")
            .auth(AuthConfig {
                auth_type: AuthType::Basic,
                credentials: Credentials {
                    username: Some("user".to_string()),
                    password: Some("pass".to_string()),
                    ..Default::default()
                },
            });

        assert_eq!(builder.config.name, "test");
        assert_eq!(builder.config.connector_type, "mqtt");
    }
}
