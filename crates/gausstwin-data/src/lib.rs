//! GaussTwin Data Layer
//!
//! High-level abstraction layer coordinating vector and traditional database operations
//! with optimized performance, robust error handling, and comprehensive metrics.
//!
//! # Features
//! - Unified interface for hybrid data operations
//! - Built-in caching and connection pooling
//! - Comprehensive metrics collection
//! - Streaming support for large result sets
//! - Batch operations for improved performance
//!
//! # Examples
//! ```no_run
//! use gausstwin_data::{UnifiedStore, UnifiedStoreConfig, create_unified_store};
//!
//! async fn example() -> anyhow::Result<()> {
//!     let config = UnifiedStoreConfig::default();
//!     let store = create_unified_store(config).await?;
//!     
//!     // Store hybrid data
//!     let key = "example";
//!     let vector = vec![1.0, 2.0, 3.0];
//!     let scalar = serde_json::json!({ "name": "test" });
//!     
//!     let id = store.store_hybrid(key, &vector, &scalar).await?;
//!     Ok(())
//! }
//! ```

use crate::error::DataResult;
use async_trait::async_trait;
use futures::Stream;
use std::sync::Arc;
use uuid::Uuid;

pub mod cache;
pub mod config;
pub mod error;
pub mod metrics;
pub mod pool;
pub mod store;
pub mod types;

// Re-exports
pub use cache::LruCache;
pub use config::StoreConfig;
pub use error::DataError;
pub use metrics::MetricsCollector;
pub use pool::*;
pub use store::DataStore;
pub use types::{
    CacheConfig, DbConfig, HybridData, HybridRecord, MetricsConfig, PoolConfig, QueryFilters,
    ScalarData, SearchResult, Value, VectorData, VectorStoreConfig,
};

/// Unified store trait for data operations
#[async_trait]
pub trait UnifiedStore: Send + Sync {
    /// Store hybrid data
    async fn store_hybrid(
        &self,
        key: &str,
        vector_data: &VectorData,
        scalar_data: &ScalarData,
    ) -> DataResult<Uuid>;

    /// Get hybrid data by key
    async fn get_hybrid(&self, key: &str) -> DataResult<HybridData>;

    /// Search hybrid data
    async fn hybrid_search(
        &self,
        vector_query: &[f32],
        scalar_filters: &QueryFilters,
        limit: usize,
    ) -> DataResult<Vec<SearchResult>>;

    /// Stream search results
    async fn stream_hybrid_search<'a>(
        &'a self,
        vector_query: &'a [f32],
        scalar_filters: &'a QueryFilters,
        batch_size: usize,
    ) -> DataResult<Box<dyn Stream<Item = DataResult<SearchResult>> + Send + 'a>>;

    /// Store multiple hybrid records
    async fn batch_store_hybrid(&self, records: &[HybridRecord]) -> DataResult<Vec<Uuid>>;

    /// Delete data by key
    async fn delete(&self, key: &str) -> DataResult<()>;

    /// List keys with prefix
    async fn list(&self, prefix: &str) -> DataResult<Vec<String>>;

    /// Search by query string
    async fn search(&self, query: &str, limit: usize) -> DataResult<Vec<Uuid>>;

    /// Get multiple values
    async fn batch_get(&self, keys: &[String]) -> DataResult<Vec<Option<String>>>;

    /// Store multiple key-value pairs
    async fn batch_put(&self, kvs: &[(String, String)]) -> DataResult<()>;

    /// Get single value
    async fn get(&self, key: &str) -> DataResult<Option<String>>;

    /// Store single key-value pair
    async fn put(&self, key: &str, value: &str) -> DataResult<()>;
}

/// Configuration for the unified store
#[derive(Debug, Clone)]
pub struct UnifiedStoreConfig {
    /// Vector store configuration
    pub vector_config: VectorStoreConfig,

    /// Database configuration
    pub db_config: DbConfig,

    /// Cache configuration (optional)
    pub cache_config: Option<CacheConfig>,

    /// Connection pool configuration
    pub pool_config: PoolConfig,

    /// Metrics configuration
    pub metrics_config: MetricsConfig,
}

impl Default for UnifiedStoreConfig {
    fn default() -> Self {
        Self {
            vector_config: VectorStoreConfig {
                dimension: 128,
                distance_type: "cosine".to_string(),
                index_type: "hnsw".to_string(),
                nprobe: 10,
                ef_construction: 200,
                ef_search: 100,
            },
            db_config: DbConfig {
                url: "localhost:8000".to_string(),
                username: "root".to_string(),
                password: "root".to_string(),
                min_connections: 1,
                max_connections: 10,
                connect_timeout: std::time::Duration::from_secs(30),
                idle_timeout: std::time::Duration::from_secs(300),
            },
            cache_config: Some(CacheConfig::default()),
            pool_config: PoolConfig {
                min_size: 1,
                max_size: 10,
                timeout_seconds: 30,
                min_idle: 1,
                max_lifetime: Some(std::time::Duration::from_secs(3600)),
                idle_timeout: Some(std::time::Duration::from_secs(300)),
                connection_timeout: std::time::Duration::from_secs(30),
            },
            metrics_config: MetricsConfig::default(),
        }
    }
}

impl UnifiedStoreConfig {
    /// Validate the configuration
    pub fn validate(&self) -> DataResult<()> {
        // Validate vector store config
        if self.vector_config.dimension == 0 {
            return Err(DataError::Config(
                "Vector dimension must be greater than 0".into(),
            ));
        }
        if self.vector_config.nprobe == 0 {
            return Err(DataError::Config("nprobe must be greater than 0".into()));
        }
        if self.vector_config.ef_construction == 0 {
            return Err(DataError::Config(
                "ef_construction must be greater than 0".into(),
            ));
        }
        if self.vector_config.ef_search == 0 {
            return Err(DataError::Config("ef_search must be greater than 0".into()));
        }

        // Validate database config
        if self.db_config.max_connections < self.db_config.min_connections {
            return Err(DataError::Config(
                "max_connections must be greater than or equal to min_connections".into(),
            ));
        }
        if self.db_config.connect_timeout.as_secs() == 0 {
            return Err(DataError::Config(
                "connect_timeout must be greater than 0".into(),
            ));
        }
        if self.db_config.idle_timeout.as_secs() == 0 {
            return Err(DataError::Config(
                "idle_timeout must be greater than 0".into(),
            ));
        }

        // Validate pool config
        if self.pool_config.max_size < self.pool_config.min_idle {
            return Err(DataError::Config(
                "pool max_size must be greater than or equal to min_idle".into(),
            ));
        }
        if self.pool_config.connection_timeout.as_secs() == 0 {
            return Err(DataError::Config(
                "pool connection_timeout must be greater than 0".into(),
            ));
        }
        if let Some(max_lifetime) = self.pool_config.max_lifetime {
            if max_lifetime.as_secs() == 0 {
                return Err(DataError::Config(
                    "pool max_lifetime must be greater than 0".into(),
                ));
            }
        }
        if let Some(idle_timeout) = self.pool_config.idle_timeout {
            if idle_timeout.as_secs() == 0 {
                return Err(DataError::Config(
                    "pool idle_timeout must be greater than 0".into(),
                ));
            }
        }

        // Validate cache config if present
        if let Some(cache_config) = &self.cache_config {
            if cache_config.max_size == 0 {
                return Err(DataError::Config(
                    "cache max_size must be greater than 0".into(),
                ));
            }
            if cache_config.ttl.as_secs() == 0 {
                return Err(DataError::Config("cache ttl must be greater than 0".into()));
            }
        }

        // Validate metrics config
        if self.metrics_config.enabled && self.metrics_config.report_interval.as_secs() == 0 {
            return Err(DataError::Config(
                "metrics report_interval must be greater than 0".into(),
            ));
        }

        Ok(())
    }
}

/// Create a new unified store instance
///
/// This function creates a fully-configured unified store with vector and scalar
/// database backends, caching layer, and connection pooling.
///
/// # Arguments
/// * `config` - Configuration for the unified store
///
/// # Returns
/// * `DataResult<impl UnifiedStore>` - A boxed unified store implementation
///
/// # Example
/// ```no_run
/// use gausstwin_data::{create_unified_store, UnifiedStoreConfig};
///
/// async fn example() -> anyhow::Result<()> {
///     let config = UnifiedStoreConfig::default();
///     let store = create_unified_store(config).await?;
///     Ok(())
/// }
/// ```
pub async fn create_unified_store(
    config: UnifiedStoreConfig,
) -> DataResult<Box<dyn UnifiedStore + Send + Sync>> {
    // Validate configuration
    config.validate()?;

    // Create in-memory vector store
    let vector_store = Arc::new(InMemoryVectorStore::new(config.vector_config.dimension));

    // Create in-memory scalar store
    let scalar_store = Arc::new(InMemoryScalarStore::new());

    // Create cache layer
    let cache: Arc<dyn cache::AsyncCache<String, HybridData>> =
        if let Some(cache_config) = &config.cache_config {
            Arc::new(cache::LruCache::new(cache_config.clone()))
        } else {
            Arc::new(cache::LruCache::new(CacheConfig::default()))
        };

    // Create unified store implementation
    let store = store::UnifiedStoreImpl::new(vector_store, scalar_store, cache, config);

    Ok(Box::new(store))
}

/// In-memory vector store implementation for testing and development
pub struct InMemoryVectorStore {
    vectors: Arc<tokio::sync::RwLock<std::collections::HashMap<String, VectorData>>>,
    dimension: usize,
}

impl InMemoryVectorStore {
    pub fn new(dimension: usize) -> Self {
        Self {
            vectors: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            dimension,
        }
    }
}

#[async_trait]
impl store::DataStore for InMemoryVectorStore {
    async fn get(&self, key: &str) -> error::DataResult<Option<serde_json::Value>> {
        let vectors = self.vectors.read().await;
        Ok(vectors
            .get(key)
            .map(|v| serde_json::to_value(v).unwrap_or_default()))
    }

    async fn set(&self, key: &str, value: serde_json::Value) -> error::DataResult<()> {
        let mut vectors = self.vectors.write().await;
        let vector_data: VectorData = serde_json::from_value(value)?;
        vectors.insert(key.to_string(), vector_data);
        Ok(())
    }

    async fn delete(&self, key: &str) -> error::DataResult<()> {
        let mut vectors = self.vectors.write().await;
        vectors.remove(key);
        Ok(())
    }

    async fn get_vector(&self, key: &str) -> error::DataResult<Option<VectorData>> {
        let vectors = self.vectors.read().await;
        Ok(vectors.get(key).cloned())
    }

    async fn get_scalar(&self, key: &str) -> error::DataResult<Option<ScalarData>> {
        let vectors = self.vectors.read().await;
        Ok(vectors.get(key).map(|v| ScalarData {
            value: v.metadata.clone(),
            metadata: serde_json::json!({}),
        }))
    }

    async fn get_hybrid(&self, key: &str) -> error::DataResult<Option<HybridData>> {
        let vectors = self.vectors.read().await;
        Ok(vectors.get(key).map(|v| HybridData {
            vector: Some(v.vector.clone()),
            value: v.metadata.clone(),
            metadata: serde_json::json!({}),
        }))
    }

    async fn search_vectors(
        &self,
        query: &VectorData,
        limit: usize,
        _filters: Option<QueryFilters>,
    ) -> error::DataResult<Vec<SearchResult>> {
        let vectors = self.vectors.read().await;
        let mut results: Vec<(String, f32, VectorData)> = vectors
            .iter()
            .map(|(k, v)| {
                let score = cosine_similarity(&query.vector, &v.vector);
                (k.clone(), score, v.clone())
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        Ok(results
            .into_iter()
            .map(|(key, score, v)| SearchResult {
                key,
                score,
                data: HybridData {
                    vector: Some(v.vector),
                    value: v.metadata,
                    metadata: serde_json::json!({}),
                },
            })
            .collect())
    }
}

/// In-memory scalar store implementation for testing and development
pub struct InMemoryScalarStore {
    data: Arc<tokio::sync::RwLock<std::collections::HashMap<String, ScalarData>>>,
}

impl InMemoryScalarStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait]
impl store::DataStore for InMemoryScalarStore {
    async fn get(&self, key: &str) -> error::DataResult<Option<serde_json::Value>> {
        let data = self.data.read().await;
        Ok(data
            .get(key)
            .map(|v| serde_json::to_value(v).unwrap_or_default()))
    }

    async fn set(&self, key: &str, value: serde_json::Value) -> error::DataResult<()> {
        let mut data = self.data.write().await;
        let scalar_data: ScalarData = serde_json::from_value(value)?;
        data.insert(key.to_string(), scalar_data);
        Ok(())
    }

    async fn delete(&self, key: &str) -> error::DataResult<()> {
        let mut data = self.data.write().await;
        data.remove(key);
        Ok(())
    }

    async fn get_vector(&self, _key: &str) -> error::DataResult<Option<VectorData>> {
        Ok(None)
    }

    async fn get_scalar(&self, key: &str) -> error::DataResult<Option<ScalarData>> {
        let data = self.data.read().await;
        Ok(data.get(key).cloned())
    }

    async fn get_hybrid(&self, key: &str) -> error::DataResult<Option<HybridData>> {
        let data = self.data.read().await;
        Ok(data.get(key).map(|v| HybridData {
            vector: None,
            value: v.value.clone(),
            metadata: v.metadata.clone(),
        }))
    }

    async fn search_vectors(
        &self,
        _query: &VectorData,
        _limit: usize,
        _filters: Option<QueryFilters>,
    ) -> error::DataResult<Vec<SearchResult>> {
        Ok(Vec::new())
    }
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    // Test fixtures and helper functions will be added here
}
