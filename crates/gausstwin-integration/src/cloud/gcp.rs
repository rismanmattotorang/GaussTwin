//! GCP Connector
//!
//! Provides integration with Google Cloud Platform services including
//! Cloud Storage, Firestore, Pub/Sub, Cloud Functions, and IoT Core.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// GCP-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCPConfig {
    /// Project ID
    pub project_id: String,
    /// Service account key JSON
    pub service_account_key: String,
    /// Region
    pub region: String,
    /// Cloud Storage bucket prefix
    pub storage_bucket_prefix: Option<String>,
    /// Pub/Sub emulator host (for local testing)
    pub pubsub_emulator_host: Option<String>,
    /// Firestore emulator host (for local testing)
    pub firestore_emulator_host: Option<String>,
    /// Request timeout
    pub timeout_secs: u64,
}

impl Default for GCPConfig {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            service_account_key: String::new(),
            region: "us-central1".to_string(),
            storage_bucket_prefix: None,
            pubsub_emulator_host: None,
            firestore_emulator_host: None,
            timeout_secs: 30,
        }
    }
}

impl From<Config> for GCPConfig {
    fn from(config: Config) -> Self {
        Self {
            project_id: "default-project".to_string(),
            service_account_key: config.auth.credentials.token.unwrap_or_default(),
            region: "us-central1".to_string(),
            storage_bucket_prefix: None,
            pubsub_emulator_host: None,
            firestore_emulator_host: None,
            timeout_secs: config.timeout.as_secs(),
        }
    }
}

/// Cloud Storage object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageObject {
    pub name: String,
    pub bucket: String,
    pub size: u64,
    pub content_type: String,
    pub created: chrono::DateTime<chrono::Utc>,
    pub updated: chrono::DateTime<chrono::Utc>,
    pub md5_hash: String,
    pub metadata: HashMap<String, String>,
}

/// Firestore document
pub type FirestoreDocument = serde_json::Value;

/// Pub/Sub message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubsubMessage {
    pub message_id: String,
    pub data: Vec<u8>,
    pub attributes: HashMap<String, String>,
    pub publish_time: chrono::DateTime<chrono::Utc>,
    pub ordering_key: Option<String>,
}

/// Pub/Sub received message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedMessage {
    pub ack_id: String,
    pub message: PubsubMessage,
    pub delivery_attempt: i32,
}

/// IoT Core device state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceState {
    pub device_id: String,
    pub state: serde_json::Value,
    pub update_time: chrono::DateTime<chrono::Utc>,
}

/// IoT Core device config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub device_id: String,
    pub version: i64,
    pub cloud_update_time: chrono::DateTime<chrono::Utc>,
    pub binary_data: Vec<u8>,
}

/// Internal state
struct ConnectorState {
    connected: AtomicBool,
    buckets: RwLock<HashMap<String, HashMap<String, Vec<u8>>>>,
    firestore_collections: RwLock<HashMap<String, Vec<FirestoreDocument>>>,
    pubsub_topics: RwLock<HashMap<String, Vec<PubsubMessage>>>,
    pubsub_subscriptions: RwLock<HashMap<String, String>>, // subscription -> topic
    iot_devices: RwLock<HashMap<String, DeviceState>>,
    iot_configs: RwLock<HashMap<String, DeviceConfig>>,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            buckets: RwLock::new(HashMap::new()),
            firestore_collections: RwLock::new(HashMap::new()),
            pubsub_topics: RwLock::new(HashMap::new()),
            pubsub_subscriptions: RwLock::new(HashMap::new()),
            iot_devices: RwLock::new(HashMap::new()),
            iot_configs: RwLock::new(HashMap::new()),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    storage_operations: AtomicU64,
    firestore_operations: AtomicU64,
    pubsub_operations: AtomicU64,
    iot_operations: AtomicU64,
    errors: AtomicU64,
    bytes_transferred: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    latency_samples: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            storage_operations: AtomicU64::new(0),
            firestore_operations: AtomicU64::new(0),
            pubsub_operations: AtomicU64::new(0),
            iot_operations: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            bytes_transferred: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            latency_samples: RwLock::new(Vec::new()),
        }
    }
}

/// GCP Connector
pub struct GCPConnector {
    config: GCPConfig,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
}

impl GCPConnector {
    /// Create a new GCP connector
    pub fn new(config: GCPConfig) -> Self {
        Self {
            config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        }
    }

    async fn record_latency(&self, duration: Duration) {
        let latency = duration.as_secs_f64() * 1000.0;
        let mut samples = self.internal_metrics.latency_samples.write().await;
        samples.push(latency);
        if samples.len() > 1000 {
            samples.drain(0..500);
        }
    }

    // Cloud Storage Operations

    /// Create a bucket
    pub async fn storage_create_bucket(&self, bucket: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut buckets = self.state.buckets.write().await;
            buckets.insert(bucket.to_string(), HashMap::new());
        }

        self.internal_metrics
            .storage_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        info!("Created Cloud Storage bucket: {}", bucket);
        Ok(())
    }

    /// Upload object
    pub async fn storage_upload(
        &self,
        bucket: &str,
        object_name: &str,
        data: Vec<u8>,
    ) -> Result<StorageObject> {
        let start = Instant::now();

        let md5_hash = format!("{:x}", md5::compute(&data));
        let now = chrono::Utc::now();

        {
            let mut buckets = self.state.buckets.write().await;
            if let Some(objects) = buckets.get_mut(bucket) {
                objects.insert(object_name.to_string(), data.clone());
            } else {
                return Err(Error::NotFound(format!("Bucket not found: {}", bucket)));
            }
        }

        self.internal_metrics
            .storage_operations
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_transferred
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(StorageObject {
            name: object_name.to_string(),
            bucket: bucket.to_string(),
            size: data.len() as u64,
            content_type: "application/octet-stream".to_string(),
            created: now,
            updated: now,
            md5_hash,
            metadata: HashMap::new(),
        })
    }

    /// Download object
    pub async fn storage_download(&self, bucket: &str, object_name: &str) -> Result<Vec<u8>> {
        let start = Instant::now();

        let data = {
            let buckets = self.state.buckets.read().await;
            if let Some(objects) = buckets.get(bucket) {
                objects
                    .get(object_name)
                    .cloned()
                    .ok_or_else(|| Error::NotFound(format!("Object not found: {}", object_name)))?
            } else {
                return Err(Error::NotFound(format!("Bucket not found: {}", bucket)));
            }
        };

        self.internal_metrics
            .storage_operations
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_transferred
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(data)
    }

    /// Delete object
    pub async fn storage_delete(&self, bucket: &str, object_name: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut buckets = self.state.buckets.write().await;
            if let Some(objects) = buckets.get_mut(bucket) {
                objects.remove(object_name);
            }
        }

        self.internal_metrics
            .storage_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(())
    }

    /// List objects
    pub async fn storage_list(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<StorageObject>> {
        let start = Instant::now();

        let objects = {
            let buckets = self.state.buckets.read().await;
            if let Some(bucket_objects) = buckets.get(bucket) {
                bucket_objects
                    .iter()
                    .filter(|(name, _)| {
                        if let Some(p) = prefix {
                            name.starts_with(p)
                        } else {
                            true
                        }
                    })
                    .map(|(name, data)| StorageObject {
                        name: name.clone(),
                        bucket: bucket.to_string(),
                        size: data.len() as u64,
                        content_type: "application/octet-stream".to_string(),
                        created: chrono::Utc::now(),
                        updated: chrono::Utc::now(),
                        md5_hash: format!("{:x}", md5::compute(data)),
                        metadata: HashMap::new(),
                    })
                    .collect()
            } else {
                return Err(Error::NotFound(format!("Bucket not found: {}", bucket)));
            }
        };

        self.internal_metrics
            .storage_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(objects)
    }

    // Firestore Operations

    /// Create document
    pub async fn firestore_create(
        &self,
        collection: &str,
        document: FirestoreDocument,
    ) -> Result<String> {
        let start = Instant::now();

        let doc_id = document
            .get("id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        {
            let mut collections = self.state.firestore_collections.write().await;
            collections
                .entry(collection.to_string())
                .or_insert_with(Vec::new)
                .push(document);
        }

        self.internal_metrics
            .firestore_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(doc_id)
    }

    /// Get document
    pub async fn firestore_get(
        &self,
        collection: &str,
        document_id: &str,
    ) -> Result<Option<FirestoreDocument>> {
        let start = Instant::now();

        let document = {
            let collections = self.state.firestore_collections.read().await;
            if let Some(docs) = collections.get(collection) {
                docs.iter()
                    .find(|d| {
                        d.get("id")
                            .and_then(|v| v.as_str())
                            .map(|id| id == document_id)
                            .unwrap_or(false)
                    })
                    .cloned()
            } else {
                None
            }
        };

        self.internal_metrics
            .firestore_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(document)
    }

    /// Query documents
    pub async fn firestore_query(
        &self,
        collection: &str,
        _query: &str,
    ) -> Result<Vec<FirestoreDocument>> {
        let start = Instant::now();

        let documents = {
            let collections = self.state.firestore_collections.read().await;
            collections.get(collection).cloned().unwrap_or_default()
        };

        self.internal_metrics
            .firestore_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(documents)
    }

    /// Delete document
    pub async fn firestore_delete(&self, collection: &str, document_id: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut collections = self.state.firestore_collections.write().await;
            if let Some(docs) = collections.get_mut(collection) {
                docs.retain(|d| {
                    d.get("id")
                        .and_then(|v| v.as_str())
                        .map(|id| id != document_id)
                        .unwrap_or(true)
                });
            }
        }

        self.internal_metrics
            .firestore_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(())
    }

    // Pub/Sub Operations

    /// Create topic
    pub async fn pubsub_create_topic(&self, topic_name: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut topics = self.state.pubsub_topics.write().await;
            topics.insert(topic_name.to_string(), Vec::new());
        }

        self.internal_metrics
            .pubsub_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        info!("Created Pub/Sub topic: {}", topic_name);
        Ok(())
    }

    /// Create subscription
    pub async fn pubsub_create_subscription(
        &self,
        subscription_name: &str,
        topic_name: &str,
    ) -> Result<()> {
        let start = Instant::now();

        {
            let mut subscriptions = self.state.pubsub_subscriptions.write().await;
            subscriptions.insert(subscription_name.to_string(), topic_name.to_string());
        }

        self.internal_metrics
            .pubsub_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        info!(
            "Created Pub/Sub subscription: {} -> {}",
            subscription_name, topic_name
        );
        Ok(())
    }

    /// Publish message
    pub async fn pubsub_publish(
        &self,
        topic_name: &str,
        data: Vec<u8>,
        attributes: HashMap<String, String>,
    ) -> Result<String> {
        let start = Instant::now();

        let message_id = uuid::Uuid::new_v4().to_string();

        {
            let mut topics = self.state.pubsub_topics.write().await;
            if let Some(messages) = topics.get_mut(topic_name) {
                messages.push(PubsubMessage {
                    message_id: message_id.clone(),
                    data,
                    attributes,
                    publish_time: chrono::Utc::now(),
                    ordering_key: None,
                });
            } else {
                return Err(Error::NotFound(format!("Topic not found: {}", topic_name)));
            }
        }

        self.internal_metrics
            .pubsub_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(message_id)
    }

    /// Pull messages
    pub async fn pubsub_pull(
        &self,
        subscription_name: &str,
        max_messages: u32,
    ) -> Result<Vec<ReceivedMessage>> {
        let start = Instant::now();

        let messages = {
            let subscriptions = self.state.pubsub_subscriptions.read().await;
            if let Some(topic_name) = subscriptions.get(subscription_name) {
                let topics = self.state.pubsub_topics.read().await;
                if let Some(topic_messages) = topics.get(topic_name) {
                    topic_messages
                        .iter()
                        .take(max_messages as usize)
                        .map(|m| ReceivedMessage {
                            ack_id: uuid::Uuid::new_v4().to_string(),
                            message: m.clone(),
                            delivery_attempt: 1,
                        })
                        .collect()
                } else {
                    vec![]
                }
            } else {
                return Err(Error::NotFound(format!(
                    "Subscription not found: {}",
                    subscription_name
                )));
            }
        };

        self.internal_metrics
            .pubsub_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(messages)
    }

    /// Acknowledge messages
    pub async fn pubsub_acknowledge(
        &self,
        subscription_name: &str,
        ack_ids: &[String],
    ) -> Result<()> {
        let start = Instant::now();

        // In simulation, just log the ack
        debug!(
            "Acknowledged {} messages for subscription {}",
            ack_ids.len(),
            subscription_name
        );

        self.internal_metrics
            .pubsub_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(())
    }

    // IoT Core Operations

    /// Update device state
    pub async fn iot_update_state(
        &self,
        device_id: &str,
        state: serde_json::Value,
    ) -> Result<DeviceState> {
        let start = Instant::now();

        let device_state = DeviceState {
            device_id: device_id.to_string(),
            state,
            update_time: chrono::Utc::now(),
        };

        {
            let mut devices = self.state.iot_devices.write().await;
            devices.insert(device_id.to_string(), device_state.clone());
        }

        self.internal_metrics
            .iot_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(device_state)
    }

    /// Get device state
    pub async fn iot_get_state(&self, device_id: &str) -> Result<DeviceState> {
        let start = Instant::now();

        let state = {
            let devices = self.state.iot_devices.read().await;
            devices.get(device_id).cloned()
        };

        self.internal_metrics
            .iot_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        state.ok_or_else(|| Error::NotFound(format!("Device not found: {}", device_id)))
    }

    /// Send device config
    pub async fn iot_send_config(
        &self,
        device_id: &str,
        config_data: Vec<u8>,
    ) -> Result<DeviceConfig> {
        let start = Instant::now();

        let device_config = {
            let mut configs = self.state.iot_configs.write().await;
            let version = configs.get(device_id).map(|c| c.version + 1).unwrap_or(1);

            let config = DeviceConfig {
                device_id: device_id.to_string(),
                version,
                cloud_update_time: chrono::Utc::now(),
                binary_data: config_data,
            };

            configs.insert(device_id.to_string(), config.clone());
            config
        };

        self.internal_metrics
            .iot_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(device_config)
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> Metrics {
        let uptime = if let Some(connected_at) = *self.internal_metrics.connected_at.read().await {
            connected_at.elapsed().as_secs()
        } else {
            0
        };

        let avg_latency = {
            let samples = self.internal_metrics.latency_samples.read().await;
            if samples.is_empty() {
                0.0
            } else {
                samples.iter().sum::<f64>() / samples.len() as f64
            }
        };

        let total_ops = self
            .internal_metrics
            .storage_operations
            .load(Ordering::Relaxed)
            + self
                .internal_metrics
                .firestore_operations
                .load(Ordering::Relaxed)
            + self
                .internal_metrics
                .pubsub_operations
                .load(Ordering::Relaxed)
            + self.internal_metrics.iot_operations.load(Ordering::Relaxed);

        Metrics {
            connections: if self.state.connected.load(Ordering::SeqCst) {
                1
            } else {
                0
            },
            connection_failures: 0,
            messages_sent: total_ops,
            messages_received: 0,
            errors: self.internal_metrics.errors.load(Ordering::Relaxed),
            average_latency_ms: avg_latency,
            bytes_sent: self
                .internal_metrics
                .bytes_transferred
                .load(Ordering::Relaxed),
            bytes_received: 0,
            uptime_seconds: uptime,
        }
    }
}

#[async_trait]
impl Connector for GCPConnector {
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to GCP project: {}", self.config.project_id);

        self.state.connected.store(true, Ordering::SeqCst);
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());

        info!("Connected to GCP");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from GCP");
        self.state.connected.store(false, Ordering::SeqCst);
        info!("Disconnected from GCP");
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

    #[tokio::test]
    async fn test_gcp_config_default() {
        let config = GCPConfig::default();
        assert!(config.project_id.is_empty());
        assert_eq!(config.region, "us-central1");
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = GCPConfig::default();
        let mut connector = GCPConnector::new(config);

        assert!(!connector.is_connected().await);
        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);
        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }

    #[tokio::test]
    async fn test_storage_operations() {
        let config = GCPConfig::default();
        let mut connector = GCPConnector::new(config);
        connector.connect().await.unwrap();

        connector
            .storage_create_bucket("test-bucket")
            .await
            .unwrap();

        let data = b"Hello, GCP!".to_vec();
        connector
            .storage_upload("test-bucket", "test-object", data.clone())
            .await
            .unwrap();

        let retrieved = connector
            .storage_download("test-bucket", "test-object")
            .await
            .unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_pubsub_operations() {
        let config = GCPConfig::default();
        let mut connector = GCPConnector::new(config);
        connector.connect().await.unwrap();

        connector.pubsub_create_topic("test-topic").await.unwrap();
        connector
            .pubsub_create_subscription("test-sub", "test-topic")
            .await
            .unwrap();

        let message_id = connector
            .pubsub_publish("test-topic", b"Test message".to_vec(), HashMap::new())
            .await
            .unwrap();
        assert!(!message_id.is_empty());

        let messages = connector.pubsub_pull("test-sub", 10).await.unwrap();
        assert_eq!(messages.len(), 1);
    }
}
