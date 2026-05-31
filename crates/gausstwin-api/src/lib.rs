//! GaussTwin API Server
//!
//! High-performance API server with support for:
//! - REST API
//! - GraphQL
//! - gRPC
//! - WebSocket
//!
//! Features:
//! - Authentication & Authorization
//! - Rate Limiting
//! - Request Validation
//! - Response Compression
//! - Metrics & Monitoring
//! - Database Integration (SurrealDB, Milvus, SkyTable)
//! - Caching
//! - Logging
//! - Error Handling

use std::sync::Arc;
use tokio::sync::RwLock;

// Public modules
pub mod api;
pub mod auth;
pub mod cache;
pub mod config;
pub mod db;
pub mod error;
pub mod graphql;
pub mod metrics;
pub mod rest;
pub mod server;
pub mod simulation_manager;
pub mod types;
pub mod utils;
pub mod websocket;

// Internal modules
mod grpc;

// Re-exports
pub use api::{ApiConfig, ApiServer};
pub use auth::{AuthManager, Claims};
pub use cache::CacheManager;
pub use config::ServerConfig;
pub use db::DatabaseManager;
pub use error::{Error, Result};
pub use metrics::MetricsManager;
pub use server::Server;
pub use simulation_manager::SimulationManager;
pub use types::*;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Database manager
    pub db: Arc<DatabaseManager>,
    /// Cache manager
    pub cache: Arc<CacheManager>,
    /// Authentication manager
    pub auth: Arc<AuthManager>,
    /// Metrics manager
    pub metrics: Arc<MetricsManager>,
    /// Simulation manager
    pub sim_manager: Arc<SimulationManager>,
    /// Configuration
    pub config: Arc<ServerConfig>,
}

impl AppState {
    /// Create a new application state
    pub async fn new(config: ServerConfig) -> Result<Self> {
        // Initialize database
        let db = Arc::new(DatabaseManager::new(&config.database).await?);

        // Initialize cache
        let cache = Arc::new(CacheManager::new(&config.cache).await?);

        // Initialize auth
        let auth = Arc::new(AuthManager::new(&config.auth)?);

        // Initialize metrics
        let metrics = Arc::new(MetricsManager::new(&config.metrics)?);

        // Initialize simulation manager
        let sim_manager = Arc::new(SimulationManager::new());

        Ok(Self {
            db,
            cache,
            auth,
            metrics,
            sim_manager,
            config: Arc::new(config),
        })
    }
}

/// Initialize the API server with the given configuration
pub async fn init(config: ServerConfig) -> Result<Server> {
    // Create application state
    let state = AppState::new(config).await?;

    // Create server instance
    let server = Server::new(state);

    Ok(server)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_init() {
        let config = ServerConfig::default();
        let server = init(config).await.unwrap();
        assert!(server.is_ready());
    }
}
