use crate::{common, Config, Connector, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleConfig {
    pub tenancy_ocid: String,
    pub user_ocid: String,
    pub fingerprint: String,
    pub private_key_path: String,
    pub region: String,
}

pub struct OracleConnector {
    config: OracleConfig,
    metrics: common::Metrics,
}

impl OracleConnector {
    pub fn new(config: OracleConfig) -> Self {
        Self {
            config,
            metrics: common::Metrics::default(),
        }
    }
}

#[async_trait]
impl Connector for OracleConnector {
    async fn connect(&mut self) -> Result<()> {
        // TODO: Implement Oracle connection
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        // TODO: Implement Oracle disconnection
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        // TODO: Implement Oracle connection check
        false
    }

    fn metrics(&self) -> &common::Metrics {
        &self.metrics
    }
}
