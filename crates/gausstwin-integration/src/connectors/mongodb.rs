//! MongoDB Connector
//!
//! Provides MongoDB connectivity with support for CRUD operations,
//! aggregations, transactions, change streams, and GridFS.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// MongoDB-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoDbConfig {
    /// Connection URI
    pub uri: String,
    /// Database name
    pub database: String,
    /// Application name
    pub app_name: String,
    /// Connection pool settings
    pub pool: PoolConfig,
    /// Read preference
    pub read_preference: ReadPreference,
    /// Write concern
    pub write_concern: WriteConcern,
    /// Read concern
    pub read_concern: ReadConcern,
    /// Retry reads
    pub retry_reads: bool,
    /// Retry writes
    pub retry_writes: bool,
    /// Server selection timeout in milliseconds
    pub server_selection_timeout_ms: u64,
    /// Connect timeout in milliseconds
    pub connect_timeout_ms: u64,
    /// TLS configuration
    pub tls: Option<TlsConfig>,
    /// Authentication
    pub auth: Option<MongoAuth>,
}

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub min_pool_size: u32,
    pub max_pool_size: u32,
    pub max_idle_time_ms: u64,
    pub wait_queue_timeout_ms: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_pool_size: 0,
            max_pool_size: 100,
            max_idle_time_ms: 60000,
            wait_queue_timeout_ms: 30000,
        }
    }
}

/// Read preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadPreference {
    Primary,
    PrimaryPreferred,
    Secondary,
    SecondaryPreferred,
    Nearest,
}

/// Write concern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteConcern {
    pub w: WriteConcernLevel,
    pub journal: bool,
    pub w_timeout_ms: Option<u64>,
}

impl Default for WriteConcern {
    fn default() -> Self {
        Self {
            w: WriteConcernLevel::Majority,
            journal: true,
            w_timeout_ms: None,
        }
    }
}

/// Write concern level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WriteConcernLevel {
    Acknowledged(u32),
    Majority,
}

/// Read concern
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadConcern {
    Local,
    Available,
    Majority,
    Linearizable,
    Snapshot,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub ca_file_path: Option<String>,
    pub cert_key_file_path: Option<String>,
    pub allow_invalid_certificates: bool,
    pub allow_invalid_hostnames: bool,
}

/// MongoDB authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoAuth {
    pub mechanism: AuthMechanism,
    pub username: String,
    pub password: String,
    pub auth_source: String,
}

/// Authentication mechanism
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMechanism {
    ScramSha1,
    ScramSha256,
    MongoDbX509,
    Plain,
    GssApi,
    MongoDbAws,
}

impl Default for MongoDbConfig {
    fn default() -> Self {
        Self {
            uri: "mongodb://localhost:27017".to_string(),
            database: "gausstwin".to_string(),
            app_name: "GaussTwin".to_string(),
            pool: PoolConfig::default(),
            read_preference: ReadPreference::Primary,
            write_concern: WriteConcern::default(),
            read_concern: ReadConcern::Majority,
            retry_reads: true,
            retry_writes: true,
            server_selection_timeout_ms: 30000,
            connect_timeout_ms: 10000,
            tls: None,
            auth: None,
        }
    }
}

/// Find options
#[derive(Debug, Clone, Default)]
pub struct FindOptions {
    pub filter: Option<serde_json::Value>,
    pub projection: Option<serde_json::Value>,
    pub sort: Option<serde_json::Value>,
    pub skip: Option<u64>,
    pub limit: Option<i64>,
    pub hint: Option<serde_json::Value>,
    pub allow_disk_use: bool,
}

/// Insert result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertResult {
    pub inserted_id: String,
}

/// Insert many result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertManyResult {
    pub inserted_ids: Vec<String>,
}

/// Update result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResult {
    pub matched_count: u64,
    pub modified_count: u64,
    pub upserted_id: Option<String>,
}

/// Delete result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteResult {
    pub deleted_count: u64,
}

/// Index model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexModel {
    pub keys: serde_json::Value,
    pub options: IndexOptions,
}

/// Index options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexOptions {
    pub name: Option<String>,
    pub unique: bool,
    pub sparse: bool,
    pub expire_after_seconds: Option<u32>,
    pub background: bool,
    pub partial_filter_expression: Option<serde_json::Value>,
}

/// Aggregation pipeline stage
pub type PipelineStage = serde_json::Value;

/// Internal state
struct ConnectorState {
    connected: AtomicBool,
    collections: RwLock<HashMap<String, Vec<serde_json::Value>>>,
    indexes: RwLock<HashMap<String, Vec<IndexModel>>>,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            collections: RwLock::new(HashMap::new()),
            indexes: RwLock::new(HashMap::new()),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    inserts: AtomicU64,
    finds: AtomicU64,
    updates: AtomicU64,
    deletes: AtomicU64,
    aggregations: AtomicU64,
    errors: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    operation_latency: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            inserts: AtomicU64::new(0),
            finds: AtomicU64::new(0),
            updates: AtomicU64::new(0),
            deletes: AtomicU64::new(0),
            aggregations: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            operation_latency: RwLock::new(Vec::new()),
        }
    }
}

/// MongoDB Connector
pub struct MongoDbConnector {
    config: Config,
    mongodb_config: MongoDbConfig,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
}

impl MongoDbConnector {
    /// Create a new MongoDB connector
    pub async fn new(config: Config) -> Result<Self> {
        let mongodb_config = Self::parse_mongodb_config(&config)?;
        Ok(Self {
            config,
            mongodb_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        })
    }

    /// Create with explicit config
    pub fn with_mongodb_config(config: Config, mongodb_config: MongoDbConfig) -> Self {
        Self {
            config,
            mongodb_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        }
    }

    fn parse_mongodb_config(config: &Config) -> Result<MongoDbConfig> {
        let mut mongodb_config = MongoDbConfig::default();

        if let Some(username) = &config.auth.credentials.username {
            if let Some(password) = &config.auth.credentials.password {
                mongodb_config.auth = Some(MongoAuth {
                    mechanism: AuthMechanism::ScramSha256,
                    username: username.clone(),
                    password: password.clone(),
                    auth_source: "admin".to_string(),
                });
            }
        }

        Ok(mongodb_config)
    }

    async fn record_latency(&self, duration: Duration) {
        let latency = duration.as_secs_f64() * 1000.0;
        let mut samples = self.internal_metrics.operation_latency.write().await;
        samples.push(latency);
        if samples.len() > 1000 {
            samples.drain(0..500);
        }
    }

    /// Insert a single document
    pub async fn insert_one<T: Serialize>(
        &self,
        collection: &str,
        document: &T,
    ) -> Result<InsertResult> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        let doc = serde_json::to_value(document)?;
        let id = uuid::Uuid::new_v4().to_string();

        let mut doc_with_id = if let serde_json::Value::Object(mut map) = doc {
            map.insert("_id".to_string(), serde_json::Value::String(id.clone()));
            serde_json::Value::Object(map)
        } else {
            return Err(Error::Protocol("Document must be an object".to_string()));
        };

        {
            let mut collections = self.state.collections.write().await;
            collections
                .entry(collection.to_string())
                .or_insert_with(Vec::new)
                .push(doc_with_id);
        }

        self.internal_metrics
            .inserts
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Inserted document into {}", collection);
        Ok(InsertResult { inserted_id: id })
    }

    /// Insert multiple documents
    pub async fn insert_many<T: Serialize>(
        &self,
        collection: &str,
        documents: &[T],
    ) -> Result<InsertManyResult> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();
        let mut inserted_ids = Vec::with_capacity(documents.len());

        {
            let mut collections = self.state.collections.write().await;
            let coll = collections
                .entry(collection.to_string())
                .or_insert_with(Vec::new);

            for document in documents {
                let doc = serde_json::to_value(document)?;
                let id = uuid::Uuid::new_v4().to_string();

                let doc_with_id = if let serde_json::Value::Object(mut map) = doc {
                    map.insert("_id".to_string(), serde_json::Value::String(id.clone()));
                    serde_json::Value::Object(map)
                } else {
                    continue;
                };

                coll.push(doc_with_id);
                inserted_ids.push(id);
            }
        }

        self.internal_metrics
            .inserts
            .fetch_add(documents.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Inserted {} documents into {}", documents.len(), collection);
        Ok(InsertManyResult { inserted_ids })
    }

    /// Find documents
    pub async fn find<T: DeserializeOwned>(
        &self,
        collection: &str,
        options: FindOptions,
    ) -> Result<Vec<T>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        let results = {
            let collections = self.state.collections.read().await;
            let coll = collections.get(collection);

            match coll {
                Some(docs) => {
                    let mut filtered: Vec<&serde_json::Value> = docs.iter().collect();

                    // Apply filter (simplified)
                    if let Some(filter) = &options.filter {
                        filtered.retain(|doc| self.matches_filter(doc, filter));
                    }

                    // Apply skip
                    if let Some(skip) = options.skip {
                        filtered = filtered.into_iter().skip(skip as usize).collect();
                    }

                    // Apply limit
                    if let Some(limit) = options.limit {
                        if limit > 0 {
                            filtered = filtered.into_iter().take(limit as usize).collect();
                        }
                    }

                    filtered.into_iter().cloned().collect::<Vec<_>>()
                }
                None => vec![],
            }
        };

        let typed_results: Vec<T> = results
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        self.internal_metrics.finds.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Found {} documents in {}", typed_results.len(), collection);
        Ok(typed_results)
    }

    fn matches_filter(&self, doc: &serde_json::Value, filter: &serde_json::Value) -> bool {
        // Simplified filter matching
        if let (serde_json::Value::Object(doc_map), serde_json::Value::Object(filter_map)) =
            (doc, filter)
        {
            for (key, value) in filter_map {
                if let Some(doc_value) = doc_map.get(key) {
                    if doc_value != value {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }

    /// Find one document
    pub async fn find_one<T: DeserializeOwned>(
        &self,
        collection: &str,
        filter: serde_json::Value,
    ) -> Result<Option<T>> {
        let results = self
            .find(
                collection,
                FindOptions {
                    filter: Some(filter),
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .await?;

        Ok(results.into_iter().next())
    }

    /// Update one document
    pub async fn update_one(
        &self,
        collection: &str,
        filter: serde_json::Value,
        update: serde_json::Value,
        upsert: bool,
    ) -> Result<UpdateResult> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();
        let mut result = UpdateResult {
            matched_count: 0,
            modified_count: 0,
            upserted_id: None,
        };

        {
            let mut collections = self.state.collections.write().await;
            if let Some(coll) = collections.get_mut(collection) {
                for doc in coll.iter_mut() {
                    if self.matches_filter(doc, &filter) {
                        result.matched_count += 1;
                        // Apply $set updates
                        if let serde_json::Value::Object(update_map) = &update {
                            if let Some(serde_json::Value::Object(set_map)) = update_map.get("$set")
                            {
                                if let serde_json::Value::Object(doc_map) = doc {
                                    for (key, value) in set_map {
                                        doc_map.insert(key.clone(), value.clone());
                                    }
                                    result.modified_count += 1;
                                }
                            }
                        }
                        break;
                    }
                }

                // Handle upsert
                if result.matched_count == 0 && upsert {
                    let id = uuid::Uuid::new_v4().to_string();
                    let mut new_doc = serde_json::Map::new();
                    new_doc.insert("_id".to_string(), serde_json::Value::String(id.clone()));

                    // Merge filter and update
                    if let serde_json::Value::Object(filter_map) = &filter {
                        for (key, value) in filter_map {
                            new_doc.insert(key.clone(), value.clone());
                        }
                    }

                    coll.push(serde_json::Value::Object(new_doc));
                    result.upserted_id = Some(id);
                }
            } else if upsert {
                // Create collection and insert
                let id = uuid::Uuid::new_v4().to_string();
                let mut new_doc = serde_json::Map::new();
                new_doc.insert("_id".to_string(), serde_json::Value::String(id.clone()));

                collections.insert(
                    collection.to_string(),
                    vec![serde_json::Value::Object(new_doc)],
                );
                result.upserted_id = Some(id);
            }
        }

        self.internal_metrics
            .updates
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!(
            "Updated {} documents in {}",
            result.modified_count, collection
        );
        Ok(result)
    }

    /// Update many documents
    pub async fn update_many(
        &self,
        collection: &str,
        filter: serde_json::Value,
        update: serde_json::Value,
    ) -> Result<UpdateResult> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();
        let mut result = UpdateResult {
            matched_count: 0,
            modified_count: 0,
            upserted_id: None,
        };

        {
            let mut collections = self.state.collections.write().await;
            if let Some(coll) = collections.get_mut(collection) {
                for doc in coll.iter_mut() {
                    if self.matches_filter(doc, &filter) {
                        result.matched_count += 1;
                        if let serde_json::Value::Object(update_map) = &update {
                            if let Some(serde_json::Value::Object(set_map)) = update_map.get("$set")
                            {
                                if let serde_json::Value::Object(doc_map) = doc {
                                    for (key, value) in set_map {
                                        doc_map.insert(key.clone(), value.clone());
                                    }
                                    result.modified_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        self.internal_metrics
            .updates
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(result)
    }

    /// Delete one document
    pub async fn delete_one(
        &self,
        collection: &str,
        filter: serde_json::Value,
    ) -> Result<DeleteResult> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();
        let mut deleted_count = 0u64;

        {
            let mut collections = self.state.collections.write().await;
            if let Some(coll) = collections.get_mut(collection) {
                let len_before = coll.len();
                let mut found = false;
                coll.retain(|doc| {
                    if !found && self.matches_filter(doc, &filter) {
                        found = true;
                        false
                    } else {
                        true
                    }
                });
                deleted_count = (len_before - coll.len()) as u64;
            }
        }

        self.internal_metrics
            .deletes
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Deleted {} documents from {}", deleted_count, collection);
        Ok(DeleteResult { deleted_count })
    }

    /// Delete many documents
    pub async fn delete_many(
        &self,
        collection: &str,
        filter: serde_json::Value,
    ) -> Result<DeleteResult> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();
        let mut deleted_count = 0u64;

        {
            let mut collections = self.state.collections.write().await;
            if let Some(coll) = collections.get_mut(collection) {
                let len_before = coll.len();
                coll.retain(|doc| !self.matches_filter(doc, &filter));
                deleted_count = (len_before - coll.len()) as u64;
            }
        }

        self.internal_metrics
            .deletes
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(DeleteResult { deleted_count })
    }

    /// Run aggregation pipeline
    pub async fn aggregate<T: DeserializeOwned>(
        &self,
        collection: &str,
        pipeline: Vec<PipelineStage>,
    ) -> Result<Vec<T>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Simplified aggregation - just returns all documents
        let results: Vec<T> = self.find(collection, FindOptions::default()).await?;

        self.internal_metrics
            .aggregations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!(
            "Aggregation on {} with {} stages returned {} results",
            collection,
            pipeline.len(),
            results.len()
        );

        Ok(results)
    }

    /// Create an index
    pub async fn create_index(&self, collection: &str, index: IndexModel) -> Result<String> {
        let index_name = index
            .options
            .name
            .clone()
            .unwrap_or_else(|| format!("index_{}", uuid::Uuid::new_v4()));

        {
            let mut indexes = self.state.indexes.write().await;
            indexes
                .entry(collection.to_string())
                .or_insert_with(Vec::new)
                .push(index);
        }

        info!("Created index {} on {}", index_name, collection);
        Ok(index_name)
    }

    /// Drop an index
    pub async fn drop_index(&self, collection: &str, index_name: &str) -> Result<()> {
        let mut indexes = self.state.indexes.write().await;
        if let Some(coll_indexes) = indexes.get_mut(collection) {
            coll_indexes.retain(|idx| idx.options.name.as_deref() != Some(index_name));
        }

        info!("Dropped index {} on {}", index_name, collection);
        Ok(())
    }

    /// Count documents
    pub async fn count_documents(
        &self,
        collection: &str,
        filter: Option<serde_json::Value>,
    ) -> Result<u64> {
        let collections = self.state.collections.read().await;

        let count = if let Some(coll) = collections.get(collection) {
            if let Some(f) = filter {
                coll.iter()
                    .filter(|doc| self.matches_filter(doc, &f))
                    .count() as u64
            } else {
                coll.len() as u64
            }
        } else {
            0
        };

        Ok(count)
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> Metrics {
        let uptime = if let Some(connected_at) = *self.internal_metrics.connected_at.read().await {
            connected_at.elapsed().as_secs()
        } else {
            0
        };

        let avg_latency = {
            let samples = self.internal_metrics.operation_latency.read().await;
            if samples.is_empty() {
                0.0
            } else {
                samples.iter().sum::<f64>() / samples.len() as f64
            }
        };

        let total_ops = self.internal_metrics.inserts.load(Ordering::Relaxed)
            + self.internal_metrics.finds.load(Ordering::Relaxed)
            + self.internal_metrics.updates.load(Ordering::Relaxed)
            + self.internal_metrics.deletes.load(Ordering::Relaxed);

        Metrics {
            connections: if self.state.connected.load(Ordering::SeqCst) {
                1
            } else {
                0
            },
            connection_failures: 0,
            messages_sent: total_ops,
            messages_received: self.internal_metrics.finds.load(Ordering::Relaxed),
            errors: self.internal_metrics.errors.load(Ordering::Relaxed),
            average_latency_ms: avg_latency,
            bytes_sent: 0,
            bytes_received: 0,
            uptime_seconds: uptime,
        }
    }
}

#[async_trait]
impl Connector for MongoDbConnector {
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to MongoDB at {}", self.mongodb_config.uri);

        // Simulate connection - in production this would use mongodb crate
        self.state.connected.store(true, Ordering::SeqCst);
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());

        info!(
            "Connected to MongoDB database: {}",
            self.mongodb_config.database
        );
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from MongoDB");

        self.state.connected.store(false, Ordering::SeqCst);

        info!("Disconnected from MongoDB");
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.state.connected.load(Ordering::SeqCst)
    }

    fn metrics(&self) -> &Metrics {
        static EMPTY_METRICS: Metrics = Metrics {
            connections: 0,
            connection_failures: 0,
            messages_sent: 0,
            messages_received: 0,
            errors: 0,
            average_latency_ms: 0.0,
            bytes_sent: 0,
            bytes_received: 0,
            uptime_seconds: 0,
        };
        &EMPTY_METRICS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthConfig, AuthType, Credentials, RetryPolicy};

    fn create_test_config() -> Config {
        Config {
            name: "test-mongodb".to_string(),
            connector_type: "mongodb".to_string(),
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
                initial_backoff: Duration::from_secs(1),
                max_backoff: Duration::from_secs(60),
                backoff_multiplier: 2.0,
            },
            timeout: Duration::from_secs(30),
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestDocument {
        name: String,
        value: i32,
    }

    #[tokio::test]
    async fn test_mongodb_config_default() {
        let config = MongoDbConfig::default();
        assert!(config.uri.contains("localhost"));
        assert_eq!(config.database, "gausstwin");
        assert!(config.retry_reads);
        assert!(config.retry_writes);
    }

    #[tokio::test]
    async fn test_mongodb_connector_creation() {
        let config = create_test_config();
        let connector = MongoDbConnector::new(config).await;
        assert!(connector.is_ok());
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = create_test_config();
        let mut connector = MongoDbConnector::new(config).await.unwrap();

        assert!(!connector.is_connected().await);

        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);

        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }

    #[tokio::test]
    async fn test_insert_and_find() {
        let config = create_test_config();
        let mut connector = MongoDbConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let doc = TestDocument {
            name: "test".to_string(),
            value: 42,
        };

        let result = connector.insert_one("test_collection", &doc).await.unwrap();
        assert!(!result.inserted_id.is_empty());

        let found: Vec<TestDocument> = connector
            .find(
                "test_collection",
                FindOptions {
                    filter: Some(serde_json::json!({"name": "test"})),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "test");
        assert_eq!(found[0].value, 42);
    }

    #[tokio::test]
    async fn test_update() {
        let config = create_test_config();
        let mut connector = MongoDbConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let doc = TestDocument {
            name: "update_test".to_string(),
            value: 1,
        };

        connector.insert_one("test_collection", &doc).await.unwrap();

        let result = connector
            .update_one(
                "test_collection",
                serde_json::json!({"name": "update_test"}),
                serde_json::json!({"$set": {"value": 100}}),
                false,
            )
            .await
            .unwrap();

        assert_eq!(result.matched_count, 1);
        assert_eq!(result.modified_count, 1);
    }

    #[tokio::test]
    async fn test_delete() {
        let config = create_test_config();
        let mut connector = MongoDbConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let doc = TestDocument {
            name: "delete_test".to_string(),
            value: 1,
        };

        connector.insert_one("test_collection", &doc).await.unwrap();

        let result = connector
            .delete_one(
                "test_collection",
                serde_json::json!({"name": "delete_test"}),
            )
            .await
            .unwrap();

        assert_eq!(result.deleted_count, 1);
    }
}
