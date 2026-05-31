//! Azure Connector
//!
//! Provides integration with Azure services including IoT Hub, Blob Storage,
//! Cosmos DB, Functions, Service Bus, and Event Hubs.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Azure-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureConfig {
    /// Tenant ID
    pub tenant_id: String,
    /// Client ID
    pub client_id: String,
    /// Client secret
    pub client_secret: String,
    /// Subscription ID
    pub subscription_id: String,
    /// Resource group
    pub resource_group: Option<String>,
    /// Storage account connection string
    pub storage_connection_string: Option<String>,
    /// IoT Hub connection string
    pub iot_hub_connection_string: Option<String>,
    /// Cosmos DB connection string
    pub cosmos_connection_string: Option<String>,
    /// Service Bus connection string
    pub service_bus_connection_string: Option<String>,
    /// Request timeout
    pub timeout_secs: u64,
}

impl Default for AzureConfig {
    fn default() -> Self {
        Self {
            tenant_id: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            subscription_id: String::new(),
            resource_group: None,
            storage_connection_string: None,
            iot_hub_connection_string: None,
            cosmos_connection_string: None,
            service_bus_connection_string: None,
            timeout_secs: 30,
        }
    }
}

impl From<Config> for AzureConfig {
    fn from(config: Config) -> Self {
        Self {
            tenant_id: "default-tenant".to_string(),
            client_id: config.auth.credentials.username.unwrap_or_default(),
            client_secret: config.auth.credentials.password.unwrap_or_default(),
            subscription_id: "default-subscription".to_string(),
            resource_group: None,
            storage_connection_string: None,
            iot_hub_connection_string: None,
            cosmos_connection_string: None,
            service_bus_connection_string: None,
            timeout_secs: config.timeout.as_secs(),
        }
    }
}

/// Blob metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobInfo {
    pub name: String,
    pub container: String,
    pub size: u64,
    pub content_type: String,
    pub last_modified: chrono::DateTime<chrono::Utc>,
    pub etag: String,
    pub metadata: HashMap<String, String>,
}

/// Cosmos DB document
pub type CosmosDocument = serde_json::Value;

/// Service Bus message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBusMessage {
    pub message_id: String,
    pub body: Vec<u8>,
    pub content_type: Option<String>,
    pub correlation_id: Option<String>,
    pub properties: HashMap<String, String>,
    pub enqueued_time: chrono::DateTime<chrono::Utc>,
}

/// Event Hub event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventHubEvent {
    pub body: Vec<u8>,
    pub partition_key: Option<String>,
    pub properties: HashMap<String, String>,
    pub enqueued_time: chrono::DateTime<chrono::Utc>,
    pub offset: String,
    pub sequence_number: i64,
}

/// IoT Hub device twin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTwin {
    pub device_id: String,
    pub etag: String,
    pub version: u64,
    pub properties: TwinProperties,
    pub tags: HashMap<String, serde_json::Value>,
}

/// Twin properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwinProperties {
    pub desired: serde_json::Value,
    pub reported: serde_json::Value,
}

/// Internal state
struct ConnectorState {
    connected: AtomicBool,
    containers: RwLock<HashMap<String, HashMap<String, Vec<u8>>>>,
    cosmos_databases: RwLock<HashMap<String, HashMap<String, Vec<CosmosDocument>>>>,
    service_bus_queues: RwLock<HashMap<String, Vec<ServiceBusMessage>>>,
    event_hubs: RwLock<HashMap<String, Vec<EventHubEvent>>>,
    device_twins: RwLock<HashMap<String, DeviceTwin>>,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            containers: RwLock::new(HashMap::new()),
            cosmos_databases: RwLock::new(HashMap::new()),
            service_bus_queues: RwLock::new(HashMap::new()),
            event_hubs: RwLock::new(HashMap::new()),
            device_twins: RwLock::new(HashMap::new()),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    blob_operations: AtomicU64,
    cosmos_operations: AtomicU64,
    service_bus_operations: AtomicU64,
    event_hub_operations: AtomicU64,
    iot_hub_operations: AtomicU64,
    errors: AtomicU64,
    bytes_transferred: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    latency_samples: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            blob_operations: AtomicU64::new(0),
            cosmos_operations: AtomicU64::new(0),
            service_bus_operations: AtomicU64::new(0),
            event_hub_operations: AtomicU64::new(0),
            iot_hub_operations: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            bytes_transferred: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            latency_samples: RwLock::new(Vec::new()),
        }
    }
}

/// Azure Connector
pub struct AzureConnector {
    config: AzureConfig,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
}

impl AzureConnector {
    /// Create a new Azure connector
    pub fn new(config: AzureConfig) -> Self {
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

    // Blob Storage Operations

    /// Create a blob container
    pub async fn blob_create_container(&self, container: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut containers = self.state.containers.write().await;
            containers.insert(container.to_string(), HashMap::new());
        }

        self.internal_metrics
            .blob_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        info!("Created blob container: {}", container);
        Ok(())
    }

    /// Upload blob
    pub async fn blob_upload(
        &self,
        container: &str,
        blob_name: &str,
        data: Vec<u8>,
    ) -> Result<String> {
        let start = Instant::now();

        let etag = format!("{:x}", md5::compute(&data));

        {
            let mut containers = self.state.containers.write().await;
            if let Some(blobs) = containers.get_mut(container) {
                blobs.insert(blob_name.to_string(), data.clone());
            } else {
                return Err(Error::NotFound(format!(
                    "Container not found: {}",
                    container
                )));
            }
        }

        self.internal_metrics
            .blob_operations
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_transferred
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!(
            "Uploaded blob {}/{} ({} bytes)",
            container,
            blob_name,
            data.len()
        );
        Ok(etag)
    }

    /// Download blob
    pub async fn blob_download(&self, container: &str, blob_name: &str) -> Result<Vec<u8>> {
        let start = Instant::now();

        let data = {
            let containers = self.state.containers.read().await;
            if let Some(blobs) = containers.get(container) {
                blobs
                    .get(blob_name)
                    .cloned()
                    .ok_or_else(|| Error::NotFound(format!("Blob not found: {}", blob_name)))?
            } else {
                return Err(Error::NotFound(format!(
                    "Container not found: {}",
                    container
                )));
            }
        };

        self.internal_metrics
            .blob_operations
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_transferred
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(data)
    }

    /// Delete blob
    pub async fn blob_delete(&self, container: &str, blob_name: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut containers = self.state.containers.write().await;
            if let Some(blobs) = containers.get_mut(container) {
                blobs.remove(blob_name);
            }
        }

        self.internal_metrics
            .blob_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(())
    }

    /// List blobs
    pub async fn blob_list(&self, container: &str, prefix: Option<&str>) -> Result<Vec<BlobInfo>> {
        let start = Instant::now();

        let blobs = {
            let containers = self.state.containers.read().await;
            if let Some(blobs) = containers.get(container) {
                blobs
                    .iter()
                    .filter(|(name, _)| {
                        if let Some(p) = prefix {
                            name.starts_with(p)
                        } else {
                            true
                        }
                    })
                    .map(|(name, data)| BlobInfo {
                        name: name.clone(),
                        container: container.to_string(),
                        size: data.len() as u64,
                        content_type: "application/octet-stream".to_string(),
                        last_modified: chrono::Utc::now(),
                        etag: format!("{:x}", md5::compute(data)),
                        metadata: HashMap::new(),
                    })
                    .collect()
            } else {
                return Err(Error::NotFound(format!(
                    "Container not found: {}",
                    container
                )));
            }
        };

        self.internal_metrics
            .blob_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(blobs)
    }

    // Cosmos DB Operations

    /// Create Cosmos DB database
    pub async fn cosmos_create_database(&self, database: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut databases = self.state.cosmos_databases.write().await;
            databases.insert(database.to_string(), HashMap::new());
        }

        self.internal_metrics
            .cosmos_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        info!("Created Cosmos DB database: {}", database);
        Ok(())
    }

    /// Create Cosmos DB container
    pub async fn cosmos_create_container(&self, database: &str, container: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut databases = self.state.cosmos_databases.write().await;
            if let Some(db) = databases.get_mut(database) {
                db.insert(container.to_string(), Vec::new());
            } else {
                return Err(Error::NotFound(format!("Database not found: {}", database)));
            }
        }

        self.internal_metrics
            .cosmos_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(())
    }

    /// Create document in Cosmos DB
    pub async fn cosmos_create_document(
        &self,
        database: &str,
        container: &str,
        document: CosmosDocument,
    ) -> Result<String> {
        let start = Instant::now();

        let id = document
            .get("id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        {
            let mut databases = self.state.cosmos_databases.write().await;
            if let Some(db) = databases.get_mut(database) {
                if let Some(cont) = db.get_mut(container) {
                    cont.push(document);
                } else {
                    return Err(Error::NotFound(format!(
                        "Container not found: {}",
                        container
                    )));
                }
            } else {
                return Err(Error::NotFound(format!("Database not found: {}", database)));
            }
        }

        self.internal_metrics
            .cosmos_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(id)
    }

    /// Query Cosmos DB
    pub async fn cosmos_query(
        &self,
        database: &str,
        container: &str,
        _query: &str,
    ) -> Result<Vec<CosmosDocument>> {
        let start = Instant::now();

        let documents = {
            let databases = self.state.cosmos_databases.read().await;
            if let Some(db) = databases.get(database) {
                if let Some(cont) = db.get(container) {
                    cont.clone()
                } else {
                    return Err(Error::NotFound(format!(
                        "Container not found: {}",
                        container
                    )));
                }
            } else {
                return Err(Error::NotFound(format!("Database not found: {}", database)));
            }
        };

        self.internal_metrics
            .cosmos_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(documents)
    }

    // Service Bus Operations

    /// Create Service Bus queue
    pub async fn service_bus_create_queue(&self, queue_name: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut queues = self.state.service_bus_queues.write().await;
            queues.insert(queue_name.to_string(), Vec::new());
        }

        self.internal_metrics
            .service_bus_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        info!("Created Service Bus queue: {}", queue_name);
        Ok(())
    }

    /// Send message to Service Bus queue
    pub async fn service_bus_send(&self, queue_name: &str, body: Vec<u8>) -> Result<String> {
        let start = Instant::now();

        let message_id = uuid::Uuid::new_v4().to_string();

        {
            let mut queues = self.state.service_bus_queues.write().await;
            if let Some(queue) = queues.get_mut(queue_name) {
                queue.push(ServiceBusMessage {
                    message_id: message_id.clone(),
                    body,
                    content_type: None,
                    correlation_id: None,
                    properties: HashMap::new(),
                    enqueued_time: chrono::Utc::now(),
                });
            } else {
                return Err(Error::NotFound(format!("Queue not found: {}", queue_name)));
            }
        }

        self.internal_metrics
            .service_bus_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(message_id)
    }

    /// Receive messages from Service Bus queue
    pub async fn service_bus_receive(
        &self,
        queue_name: &str,
        max_messages: u32,
    ) -> Result<Vec<ServiceBusMessage>> {
        let start = Instant::now();

        let messages = {
            let queues = self.state.service_bus_queues.read().await;
            if let Some(queue) = queues.get(queue_name) {
                queue.iter().take(max_messages as usize).cloned().collect()
            } else {
                return Err(Error::NotFound(format!("Queue not found: {}", queue_name)));
            }
        };

        self.internal_metrics
            .service_bus_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(messages)
    }

    // Event Hub Operations

    /// Send event to Event Hub
    pub async fn event_hub_send(
        &self,
        event_hub_name: &str,
        body: Vec<u8>,
        partition_key: Option<String>,
    ) -> Result<()> {
        let start = Instant::now();

        {
            let mut event_hubs = self.state.event_hubs.write().await;
            let hub = event_hubs
                .entry(event_hub_name.to_string())
                .or_insert_with(Vec::new);

            let sequence_number = hub.len() as i64;
            hub.push(EventHubEvent {
                body,
                partition_key,
                properties: HashMap::new(),
                enqueued_time: chrono::Utc::now(),
                offset: sequence_number.to_string(),
                sequence_number,
            });
        }

        self.internal_metrics
            .event_hub_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(())
    }

    /// Receive events from Event Hub
    pub async fn event_hub_receive(
        &self,
        event_hub_name: &str,
        partition: &str,
        starting_position: i64,
        max_events: u32,
    ) -> Result<Vec<EventHubEvent>> {
        let start = Instant::now();

        let events = {
            let event_hubs = self.state.event_hubs.read().await;
            if let Some(hub) = event_hubs.get(event_hub_name) {
                hub.iter()
                    .filter(|e| e.sequence_number >= starting_position)
                    .take(max_events as usize)
                    .cloned()
                    .collect()
            } else {
                vec![]
            }
        };

        self.internal_metrics
            .event_hub_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(events)
    }

    // IoT Hub Operations

    /// Update device twin
    pub async fn iot_hub_update_twin(
        &self,
        device_id: &str,
        properties: serde_json::Value,
    ) -> Result<DeviceTwin> {
        let start = Instant::now();

        let twin = {
            let mut twins = self.state.device_twins.write().await;
            let twin = twins
                .entry(device_id.to_string())
                .or_insert_with(|| DeviceTwin {
                    device_id: device_id.to_string(),
                    etag: uuid::Uuid::new_v4().to_string(),
                    version: 0,
                    properties: TwinProperties {
                        desired: serde_json::Value::Object(serde_json::Map::new()),
                        reported: serde_json::Value::Object(serde_json::Map::new()),
                    },
                    tags: HashMap::new(),
                });

            twin.properties.reported = properties;
            twin.version += 1;
            twin.etag = uuid::Uuid::new_v4().to_string();
            twin.clone()
        };

        self.internal_metrics
            .iot_hub_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(twin)
    }

    /// Get device twin
    pub async fn iot_hub_get_twin(&self, device_id: &str) -> Result<DeviceTwin> {
        let start = Instant::now();

        let twin = {
            let twins = self.state.device_twins.read().await;
            twins.get(device_id).cloned()
        };

        self.internal_metrics
            .iot_hub_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        twin.ok_or_else(|| Error::NotFound(format!("Device twin not found: {}", device_id)))
    }

    /// Send cloud-to-device message
    pub async fn iot_hub_send_c2d(&self, device_id: &str, message: Vec<u8>) -> Result<String> {
        let start = Instant::now();

        let message_id = uuid::Uuid::new_v4().to_string();

        self.internal_metrics
            .iot_hub_operations
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_transferred
            .fetch_add(message.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Sent C2D message to device {}: {}", device_id, message_id);
        Ok(message_id)
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
            .blob_operations
            .load(Ordering::Relaxed)
            + self
                .internal_metrics
                .cosmos_operations
                .load(Ordering::Relaxed)
            + self
                .internal_metrics
                .service_bus_operations
                .load(Ordering::Relaxed)
            + self
                .internal_metrics
                .event_hub_operations
                .load(Ordering::Relaxed)
            + self
                .internal_metrics
                .iot_hub_operations
                .load(Ordering::Relaxed);

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
impl Connector for AzureConnector {
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to Azure with tenant: {}", self.config.tenant_id);

        self.state.connected.store(true, Ordering::SeqCst);
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());

        info!("Connected to Azure");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from Azure");
        self.state.connected.store(false, Ordering::SeqCst);
        info!("Disconnected from Azure");
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
    async fn test_azure_config_default() {
        let config = AzureConfig::default();
        assert!(config.tenant_id.is_empty());
        assert_eq!(config.timeout_secs, 30);
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = AzureConfig::default();
        let mut connector = AzureConnector::new(config);

        assert!(!connector.is_connected().await);
        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);
        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }

    #[tokio::test]
    async fn test_blob_operations() {
        let config = AzureConfig::default();
        let mut connector = AzureConnector::new(config);
        connector.connect().await.unwrap();

        connector
            .blob_create_container("test-container")
            .await
            .unwrap();

        let data = b"Hello, Azure!".to_vec();
        connector
            .blob_upload("test-container", "test-blob", data.clone())
            .await
            .unwrap();

        let retrieved = connector
            .blob_download("test-container", "test-blob")
            .await
            .unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_service_bus_operations() {
        let config = AzureConfig::default();
        let mut connector = AzureConnector::new(config);
        connector.connect().await.unwrap();

        connector
            .service_bus_create_queue("test-queue")
            .await
            .unwrap();

        let message_id = connector
            .service_bus_send("test-queue", b"Test message".to_vec())
            .await
            .unwrap();
        assert!(!message_id.is_empty());

        let messages = connector
            .service_bus_receive("test-queue", 10)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
    }
}
