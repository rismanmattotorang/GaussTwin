//! GaussTwin Visualization System
//!
//! Provides advanced analytics and visualization capabilities including:
//! - Real-time dashboards
//! - Predictive analytics
//! - Prescriptive analytics
//! - What-if analysis
//! - Scenario planning

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

mod analytics;
pub mod dashboard;
mod error;
pub mod scenarios;
mod server;

pub use error::Error;
pub type Result<T> = std::result::Result<T, Error>;

/// Core visualization system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualConfig {
    /// Dashboard refresh rate in milliseconds
    pub refresh_rate: u64,
    /// Maximum number of historical data points to retain
    pub history_size: usize,
    /// Enable real-time analytics
    pub realtime_enabled: bool,
    /// Enable predictive analytics
    pub predictive_enabled: bool,
    /// Enable prescriptive analytics
    pub prescriptive_enabled: bool,
}

impl Default for VisualConfig {
    fn default() -> Self {
        Self {
            refresh_rate: 1000,
            history_size: 10000,
            realtime_enabled: true,
            predictive_enabled: true,
            prescriptive_enabled: true,
        }
    }
}

/// Main visualization system state
pub struct VisualSystem {
    config: VisualConfig,
    state: Arc<RwLock<SystemState>>,
}

#[derive(Debug)]
struct SystemState {
    dashboards: Vec<dashboard::Dashboard>,
    analytics: analytics::AnalyticsEngine,
    scenarios: scenarios::ScenarioManager,
}

impl VisualSystem {
    /// Create a new visualization system with the given configuration
    pub fn new(config: VisualConfig) -> Self {
        let state = SystemState {
            dashboards: Vec::new(),
            analytics: analytics::AnalyticsEngine::new(),
            scenarios: scenarios::ScenarioManager::new(),
        };

        Self {
            config,
            state: Arc::new(RwLock::new(state)),
        }
    }

    /// Start the visualization server
    pub async fn start_server(&self, addr: &str) -> Result<()> {
        server::start_server(addr, self.state.clone()).await
    }

    /// Create a new real-time dashboard
    pub async fn create_dashboard(
        &self,
        config: dashboard::DashboardConfig,
    ) -> Result<dashboard::DashboardId> {
        let mut state = self.state.write().await;
        let dashboard = dashboard::Dashboard::new(config);
        let id = dashboard.id();
        state.dashboards.push(dashboard);
        Ok(id)
    }

    /// Run predictive analytics on a dataset
    pub async fn run_prediction(&self, data: Vec<f64>, horizon: usize) -> Result<Vec<f64>> {
        let state = self.state.read().await;
        state.analytics.predict(data, horizon).await
    }

    /// Generate prescriptive recommendations
    pub async fn generate_recommendations(
        &self,
        context: analytics::Context,
    ) -> Result<Vec<analytics::Recommendation>> {
        let state = self.state.read().await;
        state.analytics.recommend(context).await
    }

    /// Create a new what-if scenario
    pub async fn create_scenario(
        &self,
        config: scenarios::ScenarioConfig,
    ) -> Result<scenarios::ScenarioId> {
        let mut state = self.state.write().await;
        state.scenarios.create_scenario(config).await
    }

    /// Run a what-if analysis on a scenario
    pub async fn analyze_scenario(
        &self,
        id: scenarios::ScenarioId,
    ) -> Result<scenarios::ScenarioResults> {
        let state = self.state.read().await;
        state.scenarios.analyze_scenario(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_creation() {
        let config = VisualConfig::default();
        let system = VisualSystem::new(config);
        assert!(system.state.read().await.dashboards.is_empty());
    }
}
