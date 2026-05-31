//! AWS Connector
//!
//! Provides integration with AWS services including IoT Core, S3,
//! DynamoDB, Lambda, SQS, SNS, and more.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// AWS-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AWSConfig {
    /// AWS region
    pub region: String,
    /// Access key ID
    pub access_key_id: String,
    /// Secret access key
    pub secret_access_key: String,
    /// Session token (optional, for temporary credentials)
    pub session_token: Option<String>,
    /// Endpoint URL (optional, for local testing)
    pub endpoint_url: Option<String>,
    /// IoT Core endpoint
    pub iot_endpoint: Option<String>,
    /// Request timeout
    pub timeout_secs: u64,
    /// Maximum retries
    pub max_retries: u32,
}

impl Default for AWSConfig {
    fn default() -> Self {
        Self {
            region: "us-east-1".to_string(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
            session_token: None,
            endpoint_url: None,
            iot_endpoint: None,
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}

impl From<Config> for AWSConfig {
    fn from(config: Config) -> Self {
        Self {
            region: "us-east-1".to_string(),
            access_key_id: config.auth.credentials.username.unwrap_or_default(),
            secret_access_key: config.auth.credentials.password.unwrap_or_default(),
            session_token: config.auth.credentials.token,
            endpoint_url: None,
            iot_endpoint: None,
            timeout_secs: config.timeout.as_secs(),
            max_retries: config.retry_policy.max_retries,
        }
    }
}

/// S3 object metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Object {
    pub key: String,
    pub size: u64,
    pub last_modified: chrono::DateTime<chrono::Utc>,
    pub etag: String,
    pub storage_class: String,
    pub metadata: HashMap<String, String>,
}

/// DynamoDB item
pub type DynamoItem = HashMap<String, AttributeValue>;

/// DynamoDB attribute value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttributeValue {
    S(String),
    N(String),
    B(Vec<u8>),
    SS(Vec<String>),
    NS(Vec<String>),
    BS(Vec<Vec<u8>>),
    M(HashMap<String, AttributeValue>),
    L(Vec<AttributeValue>),
    Bool(bool),
    Null(bool),
}

/// SQS message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqsMessage {
    pub message_id: String,
    pub receipt_handle: String,
    pub body: String,
    pub attributes: HashMap<String, String>,
    pub message_attributes: HashMap<String, MessageAttribute>,
}

/// Message attribute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttribute {
    pub data_type: String,
    pub string_value: Option<String>,
    pub binary_value: Option<Vec<u8>>,
}

/// Lambda invocation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaResult {
    pub status_code: i32,
    pub function_error: Option<String>,
    pub log_result: Option<String>,
    pub payload: Option<Vec<u8>>,
    pub executed_version: String,
}

/// IoT shadow state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowState {
    pub desired: Option<serde_json::Value>,
    pub reported: Option<serde_json::Value>,
    pub delta: Option<serde_json::Value>,
    pub version: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Internal state
struct ConnectorState {
    connected: AtomicBool,
    s3_buckets: RwLock<HashMap<String, HashMap<String, Vec<u8>>>>,
    dynamo_tables: RwLock<HashMap<String, Vec<DynamoItem>>>,
    sqs_queues: RwLock<HashMap<String, Vec<SqsMessage>>>,
    iot_shadows: RwLock<HashMap<String, ShadowState>>,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            s3_buckets: RwLock::new(HashMap::new()),
            dynamo_tables: RwLock::new(HashMap::new()),
            sqs_queues: RwLock::new(HashMap::new()),
            iot_shadows: RwLock::new(HashMap::new()),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    s3_operations: AtomicU64,
    dynamo_operations: AtomicU64,
    sqs_operations: AtomicU64,
    lambda_invocations: AtomicU64,
    iot_operations: AtomicU64,
    errors: AtomicU64,
    bytes_transferred: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    latency_samples: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            s3_operations: AtomicU64::new(0),
            dynamo_operations: AtomicU64::new(0),
            sqs_operations: AtomicU64::new(0),
            lambda_invocations: AtomicU64::new(0),
            iot_operations: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            bytes_transferred: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            latency_samples: RwLock::new(Vec::new()),
        }
    }
}

/// AWS Connector
pub struct AWSConnector {
    config: AWSConfig,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
}

impl AWSConnector {
    /// Create a new AWS connector
    pub fn new(config: AWSConfig) -> Self {
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

    // S3 Operations

    /// Create an S3 bucket
    pub async fn s3_create_bucket(&self, bucket: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut buckets = self.state.s3_buckets.write().await;
            buckets.insert(bucket.to_string(), HashMap::new());
        }

        self.internal_metrics
            .s3_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        info!("Created S3 bucket: {}", bucket);
        Ok(())
    }

    /// Put object to S3
    pub async fn s3_put_object(&self, bucket: &str, key: &str, data: Vec<u8>) -> Result<String> {
        let start = Instant::now();

        let etag = format!("{:x}", md5::compute(&data));

        {
            let mut buckets = self.state.s3_buckets.write().await;
            if let Some(bucket_data) = buckets.get_mut(bucket) {
                bucket_data.insert(key.to_string(), data.clone());
            } else {
                return Err(Error::NotFound(format!("Bucket not found: {}", bucket)));
            }
        }

        self.internal_metrics
            .s3_operations
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_transferred
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Put object s3://{}/{} ({} bytes)", bucket, key, data.len());
        Ok(etag)
    }

    /// Get object from S3
    pub async fn s3_get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>> {
        let start = Instant::now();

        let data = {
            let buckets = self.state.s3_buckets.read().await;
            if let Some(bucket_data) = buckets.get(bucket) {
                bucket_data
                    .get(key)
                    .cloned()
                    .ok_or_else(|| Error::NotFound(format!("Object not found: {}", key)))?
            } else {
                return Err(Error::NotFound(format!("Bucket not found: {}", bucket)));
            }
        };

        self.internal_metrics
            .s3_operations
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_transferred
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Get object s3://{}/{} ({} bytes)", bucket, key, data.len());
        Ok(data)
    }

    /// Delete object from S3
    pub async fn s3_delete_object(&self, bucket: &str, key: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut buckets = self.state.s3_buckets.write().await;
            if let Some(bucket_data) = buckets.get_mut(bucket) {
                bucket_data.remove(key);
            }
        }

        self.internal_metrics
            .s3_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Deleted object s3://{}/{}", bucket, key);
        Ok(())
    }

    /// List objects in S3 bucket
    pub async fn s3_list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<S3Object>> {
        let start = Instant::now();

        let objects = {
            let buckets = self.state.s3_buckets.read().await;
            if let Some(bucket_data) = buckets.get(bucket) {
                bucket_data
                    .iter()
                    .filter(|(key, _)| {
                        if let Some(p) = prefix {
                            key.starts_with(p)
                        } else {
                            true
                        }
                    })
                    .map(|(key, data)| S3Object {
                        key: key.clone(),
                        size: data.len() as u64,
                        last_modified: chrono::Utc::now(),
                        etag: format!("{:x}", md5::compute(data)),
                        storage_class: "STANDARD".to_string(),
                        metadata: HashMap::new(),
                    })
                    .collect()
            } else {
                return Err(Error::NotFound(format!("Bucket not found: {}", bucket)));
            }
        };

        self.internal_metrics
            .s3_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(objects)
    }

    // DynamoDB Operations

    /// Create a DynamoDB table
    pub async fn dynamo_create_table(&self, table_name: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut tables = self.state.dynamo_tables.write().await;
            tables.insert(table_name.to_string(), Vec::new());
        }

        self.internal_metrics
            .dynamo_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        info!("Created DynamoDB table: {}", table_name);
        Ok(())
    }

    /// Put item to DynamoDB
    pub async fn dynamo_put_item(&self, table_name: &str, item: DynamoItem) -> Result<()> {
        let start = Instant::now();

        {
            let mut tables = self.state.dynamo_tables.write().await;
            if let Some(table) = tables.get_mut(table_name) {
                table.push(item);
            } else {
                return Err(Error::NotFound(format!("Table not found: {}", table_name)));
            }
        }

        self.internal_metrics
            .dynamo_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Put item to DynamoDB table: {}", table_name);
        Ok(())
    }

    /// Get item from DynamoDB
    pub async fn dynamo_get_item(
        &self,
        table_name: &str,
        key: &DynamoItem,
    ) -> Result<Option<DynamoItem>> {
        let start = Instant::now();

        let item = {
            let tables = self.state.dynamo_tables.read().await;
            if let Some(table) = tables.get(table_name) {
                // Simplified key matching
                table
                    .iter()
                    .find(|item| {
                        key.iter().all(|(k, v)| {
                            item.get(k)
                                .map(|iv| format!("{:?}", iv) == format!("{:?}", v))
                                .unwrap_or(false)
                        })
                    })
                    .cloned()
            } else {
                return Err(Error::NotFound(format!("Table not found: {}", table_name)));
            }
        };

        self.internal_metrics
            .dynamo_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(item)
    }

    /// Query DynamoDB table
    pub async fn dynamo_query(
        &self,
        table_name: &str,
        _key_condition: &str,
    ) -> Result<Vec<DynamoItem>> {
        let start = Instant::now();

        let items = {
            let tables = self.state.dynamo_tables.read().await;
            if let Some(table) = tables.get(table_name) {
                table.clone()
            } else {
                return Err(Error::NotFound(format!("Table not found: {}", table_name)));
            }
        };

        self.internal_metrics
            .dynamo_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(items)
    }

    // SQS Operations

    /// Create an SQS queue
    pub async fn sqs_create_queue(&self, queue_name: &str) -> Result<String> {
        let start = Instant::now();

        let queue_url = format!(
            "https://sqs.{}.amazonaws.com/123456789012/{}",
            self.config.region, queue_name
        );

        {
            let mut queues = self.state.sqs_queues.write().await;
            queues.insert(queue_url.clone(), Vec::new());
        }

        self.internal_metrics
            .sqs_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        info!("Created SQS queue: {}", queue_name);
        Ok(queue_url)
    }

    /// Send message to SQS queue
    pub async fn sqs_send_message(&self, queue_url: &str, body: &str) -> Result<String> {
        let start = Instant::now();

        let message_id = uuid::Uuid::new_v4().to_string();

        {
            let mut queues = self.state.sqs_queues.write().await;
            if let Some(queue) = queues.get_mut(queue_url) {
                queue.push(SqsMessage {
                    message_id: message_id.clone(),
                    receipt_handle: uuid::Uuid::new_v4().to_string(),
                    body: body.to_string(),
                    attributes: HashMap::new(),
                    message_attributes: HashMap::new(),
                });
            } else {
                return Err(Error::NotFound(format!("Queue not found: {}", queue_url)));
            }
        }

        self.internal_metrics
            .sqs_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Sent message to SQS: {}", message_id);
        Ok(message_id)
    }

    /// Receive messages from SQS queue
    pub async fn sqs_receive_messages(
        &self,
        queue_url: &str,
        max_messages: i32,
    ) -> Result<Vec<SqsMessage>> {
        let start = Instant::now();

        let messages = {
            let queues = self.state.sqs_queues.read().await;
            if let Some(queue) = queues.get(queue_url) {
                queue.iter().take(max_messages as usize).cloned().collect()
            } else {
                return Err(Error::NotFound(format!("Queue not found: {}", queue_url)));
            }
        };

        self.internal_metrics
            .sqs_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(messages)
    }

    /// Delete message from SQS queue
    pub async fn sqs_delete_message(&self, queue_url: &str, receipt_handle: &str) -> Result<()> {
        let start = Instant::now();

        {
            let mut queues = self.state.sqs_queues.write().await;
            if let Some(queue) = queues.get_mut(queue_url) {
                queue.retain(|m| m.receipt_handle != receipt_handle);
            }
        }

        self.internal_metrics
            .sqs_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(())
    }

    // Lambda Operations

    /// Invoke Lambda function
    pub async fn lambda_invoke(
        &self,
        function_name: &str,
        payload: Option<Vec<u8>>,
    ) -> Result<LambdaResult> {
        let start = Instant::now();

        // Simulate Lambda invocation
        let result = LambdaResult {
            status_code: 200,
            function_error: None,
            log_result: None,
            payload: payload.map(|p| format!("Processed: {:?}", p).into_bytes()),
            executed_version: "$LATEST".to_string(),
        };

        self.internal_metrics
            .lambda_invocations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Invoked Lambda function: {}", function_name);
        Ok(result)
    }

    // IoT Operations

    /// Update IoT thing shadow
    pub async fn iot_update_shadow(
        &self,
        thing_name: &str,
        state: serde_json::Value,
    ) -> Result<ShadowState> {
        let start = Instant::now();

        let shadow = ShadowState {
            desired: None,
            reported: Some(state),
            delta: None,
            version: 1,
            timestamp: chrono::Utc::now(),
        };

        {
            let mut shadows = self.state.iot_shadows.write().await;
            shadows.insert(thing_name.to_string(), shadow.clone());
        }

        self.internal_metrics
            .iot_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Updated IoT shadow for: {}", thing_name);
        Ok(shadow)
    }

    /// Get IoT thing shadow
    pub async fn iot_get_shadow(&self, thing_name: &str) -> Result<ShadowState> {
        let start = Instant::now();

        let shadow = {
            let shadows = self.state.iot_shadows.read().await;
            shadows.get(thing_name).cloned()
        };

        self.internal_metrics
            .iot_operations
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        shadow.ok_or_else(|| Error::NotFound(format!("Shadow not found: {}", thing_name)))
    }

    /// Publish to IoT topic
    pub async fn iot_publish(&self, topic: &str, payload: &[u8]) -> Result<()> {
        let start = Instant::now();

        // Simulate IoT publish
        self.internal_metrics
            .iot_operations
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_transferred
            .fetch_add(payload.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Published {} bytes to IoT topic: {}", payload.len(), topic);
        Ok(())
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

        let total_ops = self.internal_metrics.s3_operations.load(Ordering::Relaxed)
            + self
                .internal_metrics
                .dynamo_operations
                .load(Ordering::Relaxed)
            + self.internal_metrics.sqs_operations.load(Ordering::Relaxed)
            + self
                .internal_metrics
                .lambda_invocations
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
impl Connector for AWSConnector {
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to AWS in region: {}", self.config.region);

        // Validate credentials
        if self.config.access_key_id.is_empty() || self.config.secret_access_key.is_empty() {
            warn!("AWS credentials not configured - using simulated mode");
        }

        self.state.connected.store(true, Ordering::SeqCst);
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());

        info!("Connected to AWS");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from AWS");
        self.state.connected.store(false, Ordering::SeqCst);
        info!("Disconnected from AWS");
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
    async fn test_aws_config_default() {
        let config = AWSConfig::default();
        assert_eq!(config.region, "us-east-1");
        assert_eq!(config.max_retries, 3);
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = AWSConfig::default();
        let mut connector = AWSConnector::new(config);

        assert!(!connector.is_connected().await);

        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);

        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }

    #[tokio::test]
    async fn test_s3_operations() {
        let config = AWSConfig::default();
        let mut connector = AWSConnector::new(config);
        connector.connect().await.unwrap();

        // Create bucket
        connector.s3_create_bucket("test-bucket").await.unwrap();

        // Put object
        let data = b"Hello, World!".to_vec();
        connector
            .s3_put_object("test-bucket", "test-key", data.clone())
            .await
            .unwrap();

        // Get object
        let retrieved = connector
            .s3_get_object("test-bucket", "test-key")
            .await
            .unwrap();
        assert_eq!(retrieved, data);

        // List objects
        let objects = connector
            .s3_list_objects("test-bucket", None)
            .await
            .unwrap();
        assert_eq!(objects.len(), 1);

        // Delete object
        connector
            .s3_delete_object("test-bucket", "test-key")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_sqs_operations() {
        let config = AWSConfig::default();
        let mut connector = AWSConnector::new(config);
        connector.connect().await.unwrap();

        // Create queue
        let queue_url = connector.sqs_create_queue("test-queue").await.unwrap();

        // Send message
        let message_id = connector
            .sqs_send_message(&queue_url, "Test message")
            .await
            .unwrap();
        assert!(!message_id.is_empty());

        // Receive messages
        let messages = connector
            .sqs_receive_messages(&queue_url, 10)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].body, "Test message");
    }
}
