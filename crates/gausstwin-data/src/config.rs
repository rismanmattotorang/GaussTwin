use crate::types::{CacheConfig, DbConfig, MetricsConfig, PoolConfig, VectorStoreConfig};
use serde::{Deserialize, Serialize};

/// Configuration for data stores
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreConfig {
    /// Vector store configuration
    pub vector_store: VectorStoreConfig,

    /// Database configuration
    pub database: DbConfig,

    /// Cache configuration
    pub cache: Option<CacheConfig>,

    /// Pool configuration
    pub pool: PoolConfig,

    /// Metrics configuration
    pub metrics: Option<MetricsConfig>,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            vector_store: VectorStoreConfig {
                dimension: 128,
                distance_type: "cosine".to_string(),
                index_type: "hnsw".to_string(),
                nprobe: 16,
                ef_construction: 200,
                ef_search: 32,
            },
            database: DbConfig {
                url: "postgres://localhost:5432/gausstwin".to_string(),
                username: "postgres".to_string(),
                password: "postgres".to_string(),
                min_connections: 1,
                max_connections: 10,
                connect_timeout: std::time::Duration::from_secs(30),
                idle_timeout: std::time::Duration::from_secs(300),
            },
            cache: Some(CacheConfig::default()),
            pool: PoolConfig {
                min_size: 2,
                max_size: 10,
                timeout_seconds: 30,
                min_idle: 1,
                max_lifetime: Some(std::time::Duration::from_secs(3600)),
                idle_timeout: Some(std::time::Duration::from_secs(300)),
                connection_timeout: std::time::Duration::from_secs(30),
            },
            metrics: Some(MetricsConfig::default()),
        }
    }
}
