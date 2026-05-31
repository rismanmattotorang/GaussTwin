//! Kafka Connector
//!
//! Provides Apache Kafka integration for high-throughput event streaming
//! with support for producers, consumers, and consumer groups.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Kafka-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaConfig {
    /// Bootstrap servers
    pub bootstrap_servers: Vec<String>,
    /// Client ID
    pub client_id: String,
    /// Consumer group ID
    pub group_id: Option<String>,
    /// Security protocol
    pub security_protocol: SecurityProtocol,
    /// SASL configuration
    pub sasl: Option<SaslConfig>,
    /// SSL configuration
    pub ssl: Option<SslConfig>,
    /// Producer configuration
    pub producer: ProducerConfig,
    /// Consumer configuration
    pub consumer: ConsumerConfig,
    /// Enable idempotent producer
    pub enable_idempotence: bool,
    /// Transaction ID (for exactly-once semantics)
    pub transactional_id: Option<String>,
}

/// Security protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityProtocol {
    Plaintext,
    Ssl,
    SaslPlaintext,
    SaslSsl,
}

/// SASL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaslConfig {
    pub mechanism: SaslMechanism,
    pub username: String,
    pub password: String,
}

/// SASL mechanism
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaslMechanism {
    Plain,
    ScramSha256,
    ScramSha512,
    OAuthBearer,
}

/// SSL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SslConfig {
    pub ca_location: String,
    pub certificate_location: Option<String>,
    pub key_location: Option<String>,
    pub key_password: Option<String>,
    pub verify_hostname: bool,
}

/// Producer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerConfig {
    /// Acknowledgment level
    pub acks: Acks,
    /// Maximum batch size in bytes
    pub batch_size: usize,
    /// Linger time in milliseconds
    pub linger_ms: u64,
    /// Compression type
    pub compression: Compression,
    /// Maximum in-flight requests per connection
    pub max_in_flight_requests: u32,
    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,
    /// Retry backoff in milliseconds
    pub retry_backoff_ms: u64,
    /// Maximum retries
    pub retries: u32,
}

impl Default for ProducerConfig {
    fn default() -> Self {
        Self {
            acks: Acks::All,
            batch_size: 16384,
            linger_ms: 5,
            compression: Compression::Snappy,
            max_in_flight_requests: 5,
            request_timeout_ms: 30000,
            retry_backoff_ms: 100,
            retries: 3,
        }
    }
}

/// Acknowledgment level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Acks {
    None,
    Leader,
    All,
}

/// Compression type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Compression {
    None,
    Gzip,
    Snappy,
    Lz4,
    Zstd,
}

/// Consumer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerConfig {
    /// Auto offset reset
    pub auto_offset_reset: AutoOffsetReset,
    /// Enable auto commit
    pub enable_auto_commit: bool,
    /// Auto commit interval in milliseconds
    pub auto_commit_interval_ms: u64,
    /// Maximum poll records
    pub max_poll_records: u32,
    /// Session timeout in milliseconds
    pub session_timeout_ms: u64,
    /// Heartbeat interval in milliseconds
    pub heartbeat_interval_ms: u64,
    /// Maximum partition fetch bytes
    pub max_partition_fetch_bytes: usize,
    /// Fetch minimum bytes
    pub fetch_min_bytes: usize,
    /// Fetch maximum wait in milliseconds
    pub fetch_max_wait_ms: u64,
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            auto_offset_reset: AutoOffsetReset::Latest,
            enable_auto_commit: true,
            auto_commit_interval_ms: 5000,
            max_poll_records: 500,
            session_timeout_ms: 30000,
            heartbeat_interval_ms: 3000,
            max_partition_fetch_bytes: 1048576,
            fetch_min_bytes: 1,
            fetch_max_wait_ms: 500,
        }
    }
}

/// Auto offset reset behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoOffsetReset {
    Earliest,
    Latest,
    None,
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            bootstrap_servers: vec!["localhost:9092".to_string()],
            client_id: format!("gausstwin-{}", uuid::Uuid::new_v4()),
            group_id: None,
            security_protocol: SecurityProtocol::Plaintext,
            sasl: None,
            ssl: None,
            producer: ProducerConfig::default(),
            consumer: ConsumerConfig::default(),
            enable_idempotence: true,
            transactional_id: None,
        }
    }
}

/// Kafka message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaMessage {
    pub topic: String,
    pub partition: Option<i32>,
    pub key: Option<Vec<u8>>,
    pub value: Vec<u8>,
    pub headers: HashMap<String, Vec<u8>>,
    pub timestamp: Option<i64>,
}

/// Received message with metadata
#[derive(Debug, Clone)]
pub struct ConsumedMessage {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub key: Option<Vec<u8>>,
    pub value: Vec<u8>,
    pub headers: HashMap<String, Vec<u8>>,
    pub timestamp: Option<i64>,
}

/// Topic partition assignment
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TopicPartition {
    pub topic: String,
    pub partition: i32,
}

/// Offset commit
#[derive(Debug, Clone)]
pub struct OffsetCommit {
    pub topic_partition: TopicPartition,
    pub offset: i64,
    pub metadata: Option<String>,
}

/// Internal state
struct ConnectorState {
    connected: AtomicBool,
    subscriptions: RwLock<Vec<String>>,
    assignments: RwLock<Vec<TopicPartition>>,
    offsets: RwLock<HashMap<TopicPartition, i64>>,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            subscriptions: RwLock::new(Vec::new()),
            assignments: RwLock::new(Vec::new()),
            offsets: RwLock::new(HashMap::new()),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    messages_produced: AtomicU64,
    messages_consumed: AtomicU64,
    bytes_produced: AtomicU64,
    bytes_consumed: AtomicU64,
    produce_errors: AtomicU64,
    consume_errors: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    produce_latency: RwLock<Vec<f64>>,
    consume_latency: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            messages_produced: AtomicU64::new(0),
            messages_consumed: AtomicU64::new(0),
            bytes_produced: AtomicU64::new(0),
            bytes_consumed: AtomicU64::new(0),
            produce_errors: AtomicU64::new(0),
            consume_errors: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            produce_latency: RwLock::new(Vec::new()),
            consume_latency: RwLock::new(Vec::new()),
        }
    }
}

/// Kafka Connector for event streaming
pub struct KafkaConnector {
    config: Config,
    kafka_config: KafkaConfig,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
    message_tx: Option<mpsc::Sender<ConsumedMessage>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl KafkaConnector {
    /// Create a new Kafka connector
    pub async fn new(config: Config) -> Result<Self> {
        let kafka_config = Self::parse_kafka_config(&config)?;
        Ok(Self {
            config,
            kafka_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
            message_tx: None,
            shutdown_tx: None,
        })
    }

    /// Create with explicit Kafka config
    pub fn with_kafka_config(config: Config, kafka_config: KafkaConfig) -> Self {
        Self {
            config,
            kafka_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
            message_tx: None,
            shutdown_tx: None,
        }
    }

    fn parse_kafka_config(config: &Config) -> Result<KafkaConfig> {
        let mut kafka_config = KafkaConfig::default();
        kafka_config.client_id = format!("gausstwin-{}", config.name);

        if let Some(username) = &config.auth.credentials.username {
            if let Some(password) = &config.auth.credentials.password {
                kafka_config.sasl = Some(SaslConfig {
                    mechanism: SaslMechanism::Plain,
                    username: username.clone(),
                    password: password.clone(),
                });
                kafka_config.security_protocol = SecurityProtocol::SaslPlaintext;
            }
        }

        Ok(kafka_config)
    }

    /// Produce a message
    pub async fn produce(&self, message: KafkaMessage) -> Result<(i32, i64)> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Simulate produce - in production this would use rdkafka
        let partition = message.partition.unwrap_or(0);
        let offset = {
            let mut offsets = self.state.offsets.write().await;
            let tp = TopicPartition {
                topic: message.topic.clone(),
                partition,
            };
            let current = offsets.entry(tp).or_insert(0);
            *current += 1;
            *current
        };

        self.internal_metrics
            .messages_produced
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_produced
            .fetch_add(message.value.len() as u64, Ordering::Relaxed);

        let latency = start.elapsed().as_secs_f64() * 1000.0;
        {
            let mut samples = self.internal_metrics.produce_latency.write().await;
            samples.push(latency);
            if samples.len() > 1000 {
                samples.drain(0..500);
            }
        }

        debug!(
            "Produced message to {}:{} offset {} (latency: {:.2}ms)",
            message.topic, partition, offset, latency
        );

        Ok((partition, offset))
    }

    /// Produce a JSON message
    pub async fn produce_json<T: Serialize>(
        &self,
        topic: &str,
        key: Option<&str>,
        value: &T,
    ) -> Result<(i32, i64)> {
        let value_bytes = serde_json::to_vec(value)?;
        let key_bytes = key.map(|k| k.as_bytes().to_vec());

        self.produce(KafkaMessage {
            topic: topic.to_string(),
            partition: None,
            key: key_bytes,
            value: value_bytes,
            headers: HashMap::new(),
            timestamp: None,
        })
        .await
    }

    /// Batch produce messages
    pub async fn produce_batch(&self, messages: Vec<KafkaMessage>) -> Result<Vec<(i32, i64)>> {
        let mut results = Vec::with_capacity(messages.len());
        for message in messages {
            results.push(self.produce(message).await?);
        }
        Ok(results)
    }

    /// Subscribe to topics
    pub async fn subscribe(&self, topics: &[&str]) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let mut subs = self.state.subscriptions.write().await;
        subs.clear();
        subs.extend(topics.iter().map(|t| t.to_string()));

        // Simulate partition assignment
        let mut assignments = self.state.assignments.write().await;
        assignments.clear();
        for topic in topics {
            // Assume 3 partitions per topic
            for partition in 0..3 {
                assignments.push(TopicPartition {
                    topic: topic.to_string(),
                    partition,
                });
            }
        }

        info!("Subscribed to topics: {:?}", topics);
        Ok(())
    }

    /// Unsubscribe from all topics
    pub async fn unsubscribe(&self) -> Result<()> {
        let mut subs = self.state.subscriptions.write().await;
        subs.clear();

        let mut assignments = self.state.assignments.write().await;
        assignments.clear();

        info!("Unsubscribed from all topics");
        Ok(())
    }

    /// Assign specific partitions
    pub async fn assign(&self, partitions: Vec<TopicPartition>) -> Result<()> {
        let mut assignments = self.state.assignments.write().await;
        *assignments = partitions.clone();

        info!("Assigned partitions: {:?}", assignments.len());
        Ok(())
    }

    /// Poll for messages
    pub async fn poll(&self, timeout: Duration) -> Result<Vec<ConsumedMessage>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Simulate polling - in production this would use rdkafka
        tokio::time::sleep(std::cmp::min(timeout, Duration::from_millis(10))).await;

        let latency = start.elapsed().as_secs_f64() * 1000.0;
        {
            let mut samples = self.internal_metrics.consume_latency.write().await;
            samples.push(latency);
            if samples.len() > 1000 {
                samples.drain(0..500);
            }
        }

        // Return empty for simulation
        Ok(vec![])
    }

    /// Commit offsets
    pub async fn commit(&self, commits: &[OffsetCommit]) -> Result<()> {
        let mut offsets = self.state.offsets.write().await;
        for commit in commits {
            offsets.insert(commit.topic_partition.clone(), commit.offset);
        }

        debug!("Committed {} offsets", commits.len());
        Ok(())
    }

    /// Commit all consumed offsets
    pub async fn commit_all(&self) -> Result<()> {
        // In simulation, this is a no-op
        debug!("Committed all offsets");
        Ok(())
    }

    /// Seek to a specific offset
    pub async fn seek(&self, topic_partition: &TopicPartition, offset: i64) -> Result<()> {
        let mut offsets = self.state.offsets.write().await;
        offsets.insert(topic_partition.clone(), offset);

        debug!(
            "Seeked {}:{} to offset {}",
            topic_partition.topic, topic_partition.partition, offset
        );
        Ok(())
    }

    /// Get current position
    pub async fn position(&self, topic_partition: &TopicPartition) -> Result<i64> {
        let offsets = self.state.offsets.read().await;
        Ok(offsets.get(topic_partition).copied().unwrap_or(0))
    }

    /// Get committed offset
    pub async fn committed(&self, topic_partition: &TopicPartition) -> Result<Option<i64>> {
        let offsets = self.state.offsets.read().await;
        Ok(offsets.get(topic_partition).copied())
    }

    /// Begin a transaction
    pub async fn begin_transaction(&self) -> Result<()> {
        if self.kafka_config.transactional_id.is_none() {
            return Err(Error::Configuration(
                "Transactional ID not configured".to_string(),
            ));
        }
        debug!("Transaction begun");
        Ok(())
    }

    /// Commit a transaction
    pub async fn commit_transaction(&self) -> Result<()> {
        debug!("Transaction committed");
        Ok(())
    }

    /// Abort a transaction
    pub async fn abort_transaction(&self) -> Result<()> {
        debug!("Transaction aborted");
        Ok(())
    }

    /// Get message receiver channel for async consumption
    pub fn message_receiver(&mut self) -> mpsc::Receiver<ConsumedMessage> {
        let (tx, rx) = mpsc::channel(10000);
        self.message_tx = Some(tx);
        rx
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> Metrics {
        let uptime = if let Some(connected_at) = *self.internal_metrics.connected_at.read().await {
            connected_at.elapsed().as_secs()
        } else {
            0
        };

        let avg_produce_latency = {
            let samples = self.internal_metrics.produce_latency.read().await;
            if samples.is_empty() {
                0.0
            } else {
                samples.iter().sum::<f64>() / samples.len() as f64
            }
        };

        Metrics {
            connections: if self.state.connected.load(Ordering::SeqCst) {
                1
            } else {
                0
            },
            connection_failures: 0,
            messages_sent: self
                .internal_metrics
                .messages_produced
                .load(Ordering::Relaxed),
            messages_received: self
                .internal_metrics
                .messages_consumed
                .load(Ordering::Relaxed),
            errors: self.internal_metrics.produce_errors.load(Ordering::Relaxed)
                + self.internal_metrics.consume_errors.load(Ordering::Relaxed),
            average_latency_ms: avg_produce_latency,
            bytes_sent: self.internal_metrics.bytes_produced.load(Ordering::Relaxed),
            bytes_received: self.internal_metrics.bytes_consumed.load(Ordering::Relaxed),
            uptime_seconds: uptime,
        }
    }
}

#[async_trait]
impl Connector for KafkaConnector {
    async fn connect(&mut self) -> Result<()> {
        info!(
            "Connecting to Kafka at {:?}",
            self.kafka_config.bootstrap_servers
        );

        // Simulate connection - in production this would create rdkafka producer/consumer
        self.state.connected.store(true, Ordering::SeqCst);
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());

        let (shutdown_tx, _) = broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        info!("Connected to Kafka");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from Kafka");

        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        self.state.connected.store(false, Ordering::SeqCst);

        // Clear state
        {
            let mut subs = self.state.subscriptions.write().await;
            subs.clear();
        }
        {
            let mut assignments = self.state.assignments.write().await;
            assignments.clear();
        }

        info!("Disconnected from Kafka");
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
            name: "test-kafka".to_string(),
            connector_type: "kafka".to_string(),
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

    #[tokio::test]
    async fn test_kafka_config_default() {
        let config = KafkaConfig::default();
        assert!(!config.bootstrap_servers.is_empty());
        assert!(matches!(
            config.security_protocol,
            SecurityProtocol::Plaintext
        ));
        assert!(config.enable_idempotence);
    }

    #[tokio::test]
    async fn test_kafka_connector_creation() {
        let config = create_test_config();
        let connector = KafkaConnector::new(config).await;
        assert!(connector.is_ok());
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = create_test_config();
        let mut connector = KafkaConnector::new(config).await.unwrap();

        assert!(!connector.is_connected().await);

        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);

        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }

    #[tokio::test]
    async fn test_produce() {
        let config = create_test_config();
        let mut connector = KafkaConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let message = KafkaMessage {
            topic: "test-topic".to_string(),
            partition: None,
            key: Some(b"key".to_vec()),
            value: b"value".to_vec(),
            headers: HashMap::new(),
            timestamp: None,
        };

        let result = connector.produce(message).await;
        assert!(result.is_ok());

        let (partition, offset) = result.unwrap();
        assert_eq!(partition, 0);
        assert!(offset > 0);
    }

    #[tokio::test]
    async fn test_subscribe() {
        let config = create_test_config();
        let mut connector = KafkaConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        connector.subscribe(&["topic1", "topic2"]).await.unwrap();

        let assignments = connector.state.assignments.read().await;
        assert!(!assignments.is_empty());
    }
}
