use async_trait::async_trait;
use futures::{Stream, StreamExt};
use gausstwin_db::{ComplianceConfig, SurrealStore};
use gausstwin_vec::{
    MetricType, SearchResult as VecSearchResult, Vector, VectorError,
    VectorStore as ExternalVectorStore,
};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::cache::{AsyncCache, LruCache};
use crate::config::StoreConfig;
use crate::error::{DataError, DataResult, ResourceKind, StorageError};
use crate::metrics::MetricsCollector;
use crate::pool::{ConnectionPool, PoolableConnection, PooledConnection};
use crate::types::*;
use crate::QueryFilters;
use crate::UnifiedStore;
use crate::UnifiedStoreConfig;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

/// Core data store trait for all storage operations
#[async_trait]
pub trait DataStore: Send + Sync {
    /// Get a value by key
    async fn get(&self, key: &str) -> DataResult<Option<Value>>;

    /// Set a value for a key
    async fn set(&self, key: &str, value: Value) -> DataResult<()>;

    /// Delete a value by key
    async fn delete(&self, key: &str) -> DataResult<()>;

    /// Get vector data
    async fn get_vector(&self, key: &str) -> DataResult<Option<VectorData>>;

    /// Get scalar data
    async fn get_scalar(&self, key: &str) -> DataResult<Option<ScalarData>>;

    /// Get hybrid data
    async fn get_hybrid(&self, key: &str) -> DataResult<Option<HybridData>>;

    /// Search for similar vectors
    async fn search_vectors(
        &self,
        query: &VectorData,
        limit: usize,
        filters: Option<QueryFilters>,
    ) -> DataResult<Vec<SearchResult>>;
}

/// Vector store implementation
pub struct VectorStore {
    pool: ConnectionPool<VectorStore>,
    config: StoreConfig,
    vector_client: Arc<dyn VectorStoreInterface>,
}

impl VectorStore {
    pub fn new(
        pool: ConnectionPool<VectorStore>,
        config: StoreConfig,
        vector_client: Arc<dyn VectorStoreInterface>,
    ) -> Self {
        Self {
            pool,
            config,
            vector_client,
        }
    }
}

#[async_trait]
impl PoolableConnection for VectorStore {
    async fn connect(url: &str) -> DataResult<Self> {
        // Implementation would go here
        todo!("Implement connect")
    }

    async fn close(&mut self) -> DataResult<()> {
        Ok(())
    }

    fn is_valid(&self) -> bool {
        true
    }

    async fn check_health(&self) -> DataResult<()> {
        Ok(())
    }

    async fn reset(&mut self) -> DataResult<()> {
        Ok(())
    }
}

#[async_trait]
impl DataStore for VectorStore {
    async fn get(&self, key: &str) -> DataResult<Option<Value>> {
        let vector = self.vector_client.get_vector(key).await?;
        Ok(vector.map(|v| serde_json::to_value(v).unwrap_or_default()))
    }

    async fn set(&self, key: &str, value: Value) -> DataResult<()> {
        let vector: Vector = serde_json::from_value(value)?;
        self.vector_client.insert_vector(vector).await?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> DataResult<()> {
        self.vector_client.delete_vector(key).await?;
        Ok(())
    }

    async fn get_vector(&self, key: &str) -> DataResult<Option<VectorData>> {
        let vector = self.vector_client.get_vector(key).await?;
        Ok(vector.map(|v| VectorData {
            vector: v.vector.clone(),
            metadata: v.metadata.unwrap_or_default(),
            dimension: v.vector.len(),
            namespace: "default".to_string(),
        }))
    }

    async fn get_scalar(&self, key: &str) -> DataResult<Option<ScalarData>> {
        let vector = self.vector_client.get_vector(key).await?;
        Ok(vector.map(|v| ScalarData {
            value: v.metadata.unwrap_or_default(),
            metadata: serde_json::json!({}),
        }))
    }

    async fn get_hybrid(&self, key: &str) -> DataResult<Option<HybridData>> {
        let vector = self.vector_client.get_vector(key).await?;
        Ok(vector.map(|v| HybridData {
            vector: Some(v.vector.clone()),
            value: v.metadata.unwrap_or_default(),
            metadata: serde_json::json!({}),
        }))
    }

    async fn search_vectors(
        &self,
        query: &VectorData,
        limit: usize,
        filters: Option<QueryFilters>,
    ) -> DataResult<Vec<SearchResult>> {
        let query_vector = Vector {
            id: "query".to_string(),
            vector: query.vector.clone(),
            metadata: Some(query.metadata.clone()),
        };

        let results = self
            .vector_client
            .search_vectors(query_vector, limit)
            .await?;

        Ok(results
            .into_iter()
            .map(|v| SearchResult {
                key: v.id.clone(),
                score: 0.0,
                data: HybridData {
                    vector: Some(v.vector.clone()),
                    value: v.metadata.unwrap_or_default(),
                    metadata: serde_json::json!({}),
                },
            })
            .collect())
    }
}

/// Main data store implementation coordinating vector and traditional databases
// Removed duplicate DataStore struct

/// Vector store connection implementation
pub struct VectorStoreConnection {
    client: Arc<dyn VectorStoreInterface>,
}

impl VectorStoreConnection {
    pub fn new(client: Arc<dyn VectorStoreInterface>) -> Self {
        Self { client }
    }

    pub async fn store_vector(&self, vector: &Vector) -> Result<String, VectorError> {
        self.client.insert_vector(vector.clone()).await?;
        Ok(vector.id.clone())
    }

    pub async fn get_vector(&self, key: &str) -> Result<Option<Vector>, VectorError> {
        self.client.get_vector(key).await
    }

    pub async fn search(
        &self,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, VectorError> {
        let results = self
            .client
            .search_vectors(
                Vector {
                    id: "".to_string(),
                    vector: query.to_vec(),
                    metadata: None,
                },
                limit,
            )
            .await?;
        Ok(results
            .into_iter()
            .map(|v| SearchResult {
                key: v.id.clone(),
                score: 0.0,
                data: HybridData {
                    vector: Some(v.vector.clone()),
                    value: v.metadata.unwrap_or_default(),
                    metadata: serde_json::json!({}),
                },
            })
            .collect())
    }

    pub async fn delete_vectors(&self, keys: &[String]) -> Result<(), VectorError> {
        for key in keys {
            self.client.delete_vector(key).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl PoolableConnection for VectorStoreConnection {
    async fn connect(_url: &str) -> DataResult<Self> {
        unimplemented!("VectorStoreConnection::connect is not implemented")
    }

    fn is_valid(&self) -> bool {
        true
    }

    fn ping(&self) -> bool {
        true
    }

    async fn close(&mut self) -> DataResult<()> {
        Ok(())
    }

    async fn check_health(&self) -> DataResult<()> {
        Ok(())
    }

    async fn reset(&mut self) -> DataResult<()> {
        Ok(())
    }
}

impl Deref for VectorStoreConnection {
    type Target = dyn VectorStoreInterface;

    fn deref(&self) -> &Self::Target {
        &*self.client
    }
}

impl DerefMut for VectorStoreConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::get_mut(&mut self.client).unwrap()
    }
}

/// Database connection implementation
pub struct DbConnection {
    client: Arc<dyn DataStore>,
}

impl DbConnection {
    pub fn new(client: Arc<dyn DataStore>) -> Self {
        Self { client }
    }

    pub async fn store_value(&self, key: &str, value: Value) -> DataResult<()> {
        self.client.set(key, value).await
    }

    pub async fn get_value(&self, key: &str) -> DataResult<Option<Value>> {
        self.client.get(key).await
    }

    pub async fn delete_value(&self, key: &str) -> DataResult<()> {
        self.client.delete(key).await
    }
}

#[async_trait]
impl PoolableConnection for DbConnection {
    fn is_valid(&self) -> bool {
        true
    }

    async fn connect(_url: &str) -> DataResult<Self>
    where
        Self: Sized,
    {
        Err(DataError::Other(
            "DbConnection::connect not implemented".to_string(),
        ))
    }

    async fn close(&mut self) -> DataResult<()> {
        Ok(())
    }

    async fn check_health(&self) -> DataResult<()> {
        Ok(())
    }

    async fn reset(&mut self) -> DataResult<()> {
        Ok(())
    }
}

impl Deref for DbConnection {
    type Target = dyn DataStore;

    fn deref(&self) -> &Self::Target {
        &*self.client
    }
}

impl DerefMut for DbConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::get_mut(&mut self.client).unwrap()
    }
}

/// Main unified store implementation
pub struct UnifiedStoreImpl {
    vector_store: Arc<dyn DataStore>,
    database: Arc<dyn DataStore>,
    cache: Arc<dyn AsyncCache<String, HybridData>>,
    config: UnifiedStoreConfig,
}

impl UnifiedStoreImpl {
    pub fn new(
        vector_store: Arc<dyn DataStore>,
        database: Arc<dyn DataStore>,
        cache: Arc<dyn AsyncCache<String, HybridData>>,
        config: UnifiedStoreConfig,
    ) -> Self {
        Self {
            vector_store,
            database,
            cache,
            config,
        }
    }
}

#[async_trait]
impl UnifiedStore for UnifiedStoreImpl {
    async fn store_hybrid(
        &self,
        key: &str,
        vector_data: &VectorData,
        scalar_data: &ScalarData,
    ) -> DataResult<Uuid> {
        // Store vector data
        self.vector_store
            .set(key, serde_json::to_value(vector_data)?)
            .await?;

        // Store scalar data
        self.database
            .set(key, serde_json::to_value(scalar_data)?)
            .await?;

        // Update cache
        let hybrid_data = HybridData {
            vector: Some(vector_data.vector.clone()),
            value: scalar_data.value.clone(),
            metadata: serde_json::json!({
                "vector_metadata": vector_data.metadata,
                "scalar_metadata": scalar_data.metadata,
            }),
        };

        self.cache.put(key.to_string(), hybrid_data, None).await?;

        Ok(Uuid::new_v4())
    }

    async fn get_hybrid(&self, key: &str) -> DataResult<HybridData> {
        // Try cache first
        if let Ok(Some(data)) = self.cache.get(&key.to_string()).await {
            return Ok(data);
        }

        // Get vector and scalar data
        let vector_data = self.vector_store.get_vector(key).await?;
        let scalar_data = self.database.get_scalar(key).await?;

        match (vector_data, scalar_data) {
            (Some(vector), Some(scalar)) => {
                let hybrid_data = HybridData {
                    vector: Some(vector.vector.clone()),
                    value: scalar.value,
                    metadata: serde_json::json!({
                        "vector_metadata": vector.metadata,
                        "scalar_metadata": scalar.metadata,
                    }),
                };

                // Update cache
                self.cache
                    .put(key.to_string(), hybrid_data.clone(), None)
                    .await?;

                Ok(hybrid_data)
            }
            _ => Err(DataError::not_found(ResourceKind::Hybrid, key)),
        }
    }

    async fn hybrid_search(
        &self,
        vector_query: &[f32],
        scalar_filters: &QueryFilters,
        limit: usize,
    ) -> DataResult<Vec<SearchResult>> {
        let query_data = VectorData {
            vector: vector_query.to_vec(),
            metadata: serde_json::json!({}),
            dimension: vector_query.len(),
            namespace: "default".to_string(),
        };

        let results = self
            .vector_store
            .search_vectors(&query_data, limit, Some(scalar_filters.clone()))
            .await?;

        // Enrich results with scalar data
        let mut enriched_results = Vec::with_capacity(results.len());
        for result in results {
            if let Ok(Some(scalar_data)) = self.database.get_scalar(&result.key).await {
                let hybrid_data = HybridData {
                    vector: result.data.vector.clone(),
                    value: scalar_data.value,
                    metadata: serde_json::json!({
                        "vector_metadata": result.data.metadata,
                        "scalar_metadata": scalar_data.metadata,
                    }),
                };

                enriched_results.push(SearchResult {
                    key: result.key,
                    score: result.score,
                    data: hybrid_data,
                });
            }
        }

        Ok(enriched_results)
    }

    async fn stream_hybrid_search<'a>(
        &'a self,
        vector_query: &'a [f32],
        scalar_filters: &'a QueryFilters,
        batch_size: usize,
    ) -> DataResult<Box<dyn Stream<Item = DataResult<SearchResult>> + Send + 'a>> {
        let results = self
            .hybrid_search(vector_query, scalar_filters, batch_size)
            .await?;
        Ok(Box::new(futures::stream::iter(results.into_iter().map(Ok))))
    }

    async fn batch_store_hybrid(&self, records: &[HybridRecord]) -> DataResult<Vec<Uuid>> {
        let mut ids = Vec::with_capacity(records.len());

        for record in records {
            if let Some(vec_vals) = &record.data.vector {
                let vector_data = VectorData {
                    vector: vec_vals.clone(),
                    metadata: serde_json::json!({}),
                    dimension: vec_vals.len(),
                    namespace: "default".to_string(),
                };

                let id = self
                    .store_hybrid(
                        &record.key,
                        &vector_data,
                        &ScalarData {
                            value: record.data.value.clone(),
                            metadata: record.data.metadata.clone(),
                        },
                    )
                    .await?;
                ids.push(id);
            }
        }

        Ok(ids)
    }

    async fn delete(&self, key: &str) -> DataResult<()> {
        // Delete from both stores
        self.vector_store.delete(key).await?;
        self.database.delete(key).await?;

        // Remove from cache
        self.cache.remove(&key.to_string()).await?;

        Ok(())
    }

    async fn list(&self, prefix: &str) -> DataResult<Vec<String>> {
        // List from vector store
        let mut keys = Vec::new();
        if let Ok(Some(value)) = self.vector_store.get(prefix).await {
            if let Ok(vector_keys) = serde_json::from_value::<Vec<String>>(value) {
                keys.extend(vector_keys);
            }
        }
        Ok(keys)
    }

    async fn search(&self, query: &str, limit: usize) -> DataResult<Vec<Uuid>> {
        // Parse query as vector if possible
        if let Ok(vector) = serde_json::from_str::<Vec<f32>>(query) {
            let results = self
                .hybrid_search(
                    &vector,
                    &QueryFilters {
                        metadata_filters: None,
                        value_filters: None,
                    },
                    limit,
                )
                .await?;

            Ok(results
                .into_iter()
                .map(|r| Uuid::parse_str(&r.key))
                .collect::<Result<Vec<_>, _>>()?)
        } else {
            Ok(Vec::new())
        }
    }

    async fn batch_get(&self, keys: &[String]) -> DataResult<Vec<Option<String>>> {
        let mut results = Vec::with_capacity(keys.len());
        for key in keys {
            let result = match self.get_hybrid(key).await {
                Ok(data) => Some(serde_json::to_string(&data)?),
                Err(_) => None,
            };
            results.push(result);
        }
        Ok(results)
    }

    async fn batch_put(&self, kvs: &[(String, String)]) -> DataResult<()> {
        for (key, value) in kvs {
            let data: HybridData = serde_json::from_str(value)?;
            if let Some(vec_vals) = data.vector {
                let dim = vec_vals.len();
                self.store_hybrid(
                    key,
                    &VectorData {
                        vector: vec_vals,
                        metadata: data.metadata.clone(),
                        dimension: dim,
                        namespace: "default".to_string(),
                    },
                    &ScalarData {
                        value: data.value,
                        metadata: data.metadata,
                    },
                )
                .await?;
            }
        }
        Ok(())
    }

    // --- Simple key/value helpers -----------------------------------------------------------

    async fn get(&self, key: &str) -> DataResult<Option<String>> {
        match self.get_hybrid(key).await {
            Ok(data) => Ok(Some(serde_json::to_string(&data)?)),
            Err(DataError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn put(&self, key: &str, value: &str) -> DataResult<()> {
        let data: HybridData = serde_json::from_str(value)?;
        if let Some(vec_vals) = &data.vector {
            let vector_data = VectorData {
                vector: vec_vals.clone(),
                metadata: data.metadata.clone(),
                dimension: vec_vals.len(),
                namespace: "default".to_string(),
            };

            self.store_hybrid(
                key,
                &vector_data,
                &ScalarData {
                    value: data.value,
                    metadata: data.metadata,
                },
            )
            .await?;
        }
        Ok(())
    }
}

#[async_trait]
pub trait VectorStoreInterface: Send + Sync {
    async fn insert_vector(&self, vector: Vector) -> Result<(), VectorError>;
    async fn search_vectors(&self, query: Vector, limit: usize)
        -> Result<Vec<Vector>, VectorError>;
    async fn delete_vector(&self, id: &str) -> Result<(), VectorError>;
    async fn get_vector(&self, id: &str) -> Result<Option<Vector>, VectorError>;
}

/// Vector store client for external vector stores
pub struct VectorStoreClient {
    client: Arc<dyn VectorStoreInterface>,
}

impl VectorStoreClient {
    pub fn new(client: Arc<dyn VectorStoreInterface>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl PoolableConnection for VectorStoreClient {
    fn is_valid(&self) -> bool {
        true
    }

    async fn connect(_url: &str) -> DataResult<Self>
    where
        Self: Sized,
    {
        Err(DataError::Other(
            "VectorStoreClient::connect not implemented".to_string(),
        ))
    }

    async fn close(&mut self) -> DataResult<()> {
        Ok(())
    }

    async fn check_health(&self) -> DataResult<()> {
        Ok(())
    }

    async fn reset(&mut self) -> DataResult<()> {
        Ok(())
    }
}

impl std::ops::Deref for VectorStoreClient {
    type Target = dyn VectorStoreInterface;

    fn deref(&self) -> &Self::Target {
        &*self.client
    }
}
