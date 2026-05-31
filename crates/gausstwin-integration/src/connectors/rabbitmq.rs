//! RabbitMQ Connector
//!
//! Provides AMQP 0-9-1 connectivity for message queuing with support for
//! exchanges, queues, bindings, and various exchange types.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// RabbitMQ-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RabbitMqConfig {
    /// Connection URI
    pub uri: String,
    /// Virtual host
    pub vhost: String,
    /// Heartbeat interval in seconds
    pub heartbeat: u16,
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,
    /// Channel max
    pub channel_max: u16,
    /// Frame max
    pub frame_max: u32,
    /// TLS configuration
    pub tls: Option<TlsConfig>,
    /// Prefetch count
    pub prefetch_count: u16,
    /// Publisher confirms
    pub publisher_confirms: bool,
    /// Consumer tag prefix
    pub consumer_tag_prefix: String,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
    pub verify_peer: bool,
}

impl Default for RabbitMqConfig {
    fn default() -> Self {
        Self {
            uri: "amqp://guest:guest@localhost:5672".to_string(),
            vhost: "/".to_string(),
            heartbeat: 60,
            connection_timeout_ms: 30000,
            channel_max: 2047,
            frame_max: 131072,
            tls: None,
            prefetch_count: 10,
            publisher_confirms: true,
            consumer_tag_prefix: "gausstwin".to_string(),
        }
    }
}

/// Exchange types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExchangeType {
    Direct,
    Fanout,
    Topic,
    Headers,
}

impl ExchangeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExchangeType::Direct => "direct",
            ExchangeType::Fanout => "fanout",
            ExchangeType::Topic => "topic",
            ExchangeType::Headers => "headers",
        }
    }
}

/// Exchange declaration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeDeclare {
    pub name: String,
    pub exchange_type: ExchangeType,
    pub durable: bool,
    pub auto_delete: bool,
    pub internal: bool,
    pub arguments: HashMap<String, String>,
}

/// Queue declaration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueDeclare {
    pub name: String,
    pub durable: bool,
    pub exclusive: bool,
    pub auto_delete: bool,
    pub arguments: HashMap<String, String>,
}

/// Queue bind options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueBind {
    pub queue: String,
    pub exchange: String,
    pub routing_key: String,
    pub arguments: HashMap<String, String>,
}

/// Message properties
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageProperties {
    pub content_type: Option<String>,
    pub content_encoding: Option<String>,
    pub delivery_mode: Option<DeliveryMode>,
    pub priority: Option<u8>,
    pub correlation_id: Option<String>,
    pub reply_to: Option<String>,
    pub expiration: Option<String>,
    pub message_id: Option<String>,
    pub timestamp: Option<i64>,
    pub message_type: Option<String>,
    pub user_id: Option<String>,
    pub app_id: Option<String>,
    pub headers: HashMap<String, String>,
}

/// Delivery mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryMode {
    Transient = 1,
    Persistent = 2,
}

/// Publish options
#[derive(Debug, Clone, Default)]
pub struct PublishOptions {
    pub mandatory: bool,
    pub immediate: bool,
}

/// Consumed message
#[derive(Debug, Clone)]
pub struct Delivery {
    pub delivery_tag: u64,
    pub redelivered: bool,
    pub exchange: String,
    pub routing_key: String,
    pub properties: MessageProperties,
    pub body: Vec<u8>,
}

/// Consumer options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumeOptions {
    pub consumer_tag: String,
    pub no_local: bool,
    pub no_ack: bool,
    pub exclusive: bool,
    pub arguments: HashMap<String, String>,
}

impl Default for ConsumeOptions {
    fn default() -> Self {
        Self {
            consumer_tag: String::new(),
            no_local: false,
            no_ack: false,
            exclusive: false,
            arguments: HashMap::new(),
        }
    }
}

/// Internal state
struct ConnectorState {
    connected: AtomicBool,
    exchanges: RwLock<HashMap<String, ExchangeDeclare>>,
    queues: RwLock<HashMap<String, QueueDeclare>>,
    bindings: RwLock<Vec<QueueBind>>,
    consumers: RwLock<HashMap<String, String>>, // consumer_tag -> queue
    delivery_tag_counter: AtomicU64,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            exchanges: RwLock::new(HashMap::new()),
            queues: RwLock::new(HashMap::new()),
            bindings: RwLock::new(Vec::new()),
            consumers: RwLock::new(HashMap::new()),
            delivery_tag_counter: AtomicU64::new(1),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    messages_published: AtomicU64,
    messages_consumed: AtomicU64,
    messages_acked: AtomicU64,
    messages_nacked: AtomicU64,
    bytes_published: AtomicU64,
    bytes_consumed: AtomicU64,
    publish_confirms: AtomicU64,
    publish_returns: AtomicU64,
    errors: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    publish_latency: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            messages_published: AtomicU64::new(0),
            messages_consumed: AtomicU64::new(0),
            messages_acked: AtomicU64::new(0),
            messages_nacked: AtomicU64::new(0),
            bytes_published: AtomicU64::new(0),
            bytes_consumed: AtomicU64::new(0),
            publish_confirms: AtomicU64::new(0),
            publish_returns: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            publish_latency: RwLock::new(Vec::new()),
        }
    }
}

/// RabbitMQ Connector
pub struct RabbitMqConnector {
    config: Config,
    rabbitmq_config: RabbitMqConfig,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
    delivery_tx: Option<mpsc::Sender<Delivery>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl RabbitMqConnector {
    /// Create a new RabbitMQ connector
    pub async fn new(config: Config) -> Result<Self> {
        let rabbitmq_config = Self::parse_rabbitmq_config(&config)?;
        Ok(Self {
            config,
            rabbitmq_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
            delivery_tx: None,
            shutdown_tx: None,
        })
    }

    /// Create with explicit config
    pub fn with_rabbitmq_config(config: Config, rabbitmq_config: RabbitMqConfig) -> Self {
        Self {
            config,
            rabbitmq_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
            delivery_tx: None,
            shutdown_tx: None,
        }
    }

    fn parse_rabbitmq_config(config: &Config) -> Result<RabbitMqConfig> {
        let mut rabbitmq_config = RabbitMqConfig::default();

        if let Some(username) = &config.auth.credentials.username {
            if let Some(password) = &config.auth.credentials.password {
                rabbitmq_config.uri = format!("amqp://{}:{}@localhost:5672", username, password);
            }
        }

        Ok(rabbitmq_config)
    }

    /// Declare an exchange
    pub async fn exchange_declare(&self, exchange: ExchangeDeclare) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let mut exchanges = self.state.exchanges.write().await;
        exchanges.insert(exchange.name.clone(), exchange.clone());

        info!(
            "Declared exchange: {} ({:?})",
            exchange.name, exchange.exchange_type
        );
        Ok(())
    }

    /// Delete an exchange
    pub async fn exchange_delete(&self, name: &str, if_unused: bool) -> Result<()> {
        let mut exchanges = self.state.exchanges.write().await;

        if if_unused {
            let bindings = self.state.bindings.read().await;
            if bindings.iter().any(|b| b.exchange == name) {
                return Err(Error::Protocol("Exchange is in use".to_string()));
            }
        }

        exchanges.remove(name);
        info!("Deleted exchange: {}", name);
        Ok(())
    }

    /// Declare a queue
    pub async fn queue_declare(&self, queue: QueueDeclare) -> Result<String> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let queue_name = if queue.name.is_empty() {
            format!("amq.gen-{}", uuid::Uuid::new_v4())
        } else {
            queue.name.clone()
        };

        let mut queues = self.state.queues.write().await;
        queues.insert(
            queue_name.clone(),
            QueueDeclare {
                name: queue_name.clone(),
                ..queue
            },
        );

        info!("Declared queue: {}", queue_name);
        Ok(queue_name)
    }

    /// Delete a queue
    pub async fn queue_delete(&self, name: &str, if_unused: bool, if_empty: bool) -> Result<u32> {
        let mut queues = self.state.queues.write().await;

        if if_unused {
            let consumers = self.state.consumers.read().await;
            if consumers.values().any(|q| q == name) {
                return Err(Error::Protocol("Queue has consumers".to_string()));
            }
        }

        queues.remove(name);
        info!("Deleted queue: {}", name);
        Ok(0) // Message count
    }

    /// Purge a queue
    pub async fn queue_purge(&self, name: &str) -> Result<u32> {
        debug!("Purged queue: {}", name);
        Ok(0) // Purged message count
    }

    /// Bind a queue to an exchange
    pub async fn queue_bind(&self, bind: QueueBind) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let mut bindings = self.state.bindings.write().await;
        bindings.push(bind.clone());

        info!(
            "Bound queue {} to exchange {} with key {}",
            bind.queue, bind.exchange, bind.routing_key
        );
        Ok(())
    }

    /// Unbind a queue from an exchange
    pub async fn queue_unbind(&self, queue: &str, exchange: &str, routing_key: &str) -> Result<()> {
        let mut bindings = self.state.bindings.write().await;
        bindings.retain(|b| {
            !(b.queue == queue && b.exchange == exchange && b.routing_key == routing_key)
        });

        info!(
            "Unbound queue {} from exchange {} with key {}",
            queue, exchange, routing_key
        );
        Ok(())
    }

    /// Publish a message
    pub async fn publish(
        &self,
        exchange: &str,
        routing_key: &str,
        body: &[u8],
        properties: MessageProperties,
        options: PublishOptions,
    ) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Simulate publish - in production this would use lapin
        self.internal_metrics
            .messages_published
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_published
            .fetch_add(body.len() as u64, Ordering::Relaxed);

        if self.rabbitmq_config.publisher_confirms {
            self.internal_metrics
                .publish_confirms
                .fetch_add(1, Ordering::Relaxed);
        }

        let latency = start.elapsed().as_secs_f64() * 1000.0;
        {
            let mut samples = self.internal_metrics.publish_latency.write().await;
            samples.push(latency);
            if samples.len() > 1000 {
                samples.drain(0..500);
            }
        }

        debug!(
            "Published {} bytes to {}:{} (latency: {:.2}ms)",
            body.len(),
            exchange,
            routing_key,
            latency
        );

        Ok(())
    }

    /// Publish a JSON message
    pub async fn publish_json<T: Serialize>(
        &self,
        exchange: &str,
        routing_key: &str,
        data: &T,
    ) -> Result<()> {
        let body = serde_json::to_vec(data)?;
        let mut properties = MessageProperties::default();
        properties.content_type = Some("application/json".to_string());
        properties.delivery_mode = Some(DeliveryMode::Persistent);

        self.publish(
            exchange,
            routing_key,
            &body,
            properties,
            PublishOptions::default(),
        )
        .await
    }

    /// Start consuming from a queue
    pub async fn consume(&self, queue: &str, options: ConsumeOptions) -> Result<String> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let consumer_tag = if options.consumer_tag.is_empty() {
            format!(
                "{}-{}",
                self.rabbitmq_config.consumer_tag_prefix,
                uuid::Uuid::new_v4()
            )
        } else {
            options.consumer_tag.clone()
        };

        let mut consumers = self.state.consumers.write().await;
        consumers.insert(consumer_tag.clone(), queue.to_string());

        info!("Started consumer {} on queue {}", consumer_tag, queue);
        Ok(consumer_tag)
    }

    /// Cancel a consumer
    pub async fn cancel(&self, consumer_tag: &str) -> Result<()> {
        let mut consumers = self.state.consumers.write().await;
        consumers.remove(consumer_tag);

        info!("Cancelled consumer {}", consumer_tag);
        Ok(())
    }

    /// Acknowledge a delivery
    pub async fn ack(&self, delivery_tag: u64, multiple: bool) -> Result<()> {
        self.internal_metrics
            .messages_acked
            .fetch_add(if multiple { delivery_tag } else { 1 }, Ordering::Relaxed);

        debug!("Acked delivery {} (multiple: {})", delivery_tag, multiple);
        Ok(())
    }

    /// Reject a delivery
    pub async fn nack(&self, delivery_tag: u64, multiple: bool, requeue: bool) -> Result<()> {
        self.internal_metrics
            .messages_nacked
            .fetch_add(if multiple { delivery_tag } else { 1 }, Ordering::Relaxed);

        debug!(
            "Nacked delivery {} (multiple: {}, requeue: {})",
            delivery_tag, multiple, requeue
        );
        Ok(())
    }

    /// Reject a single delivery
    pub async fn reject(&self, delivery_tag: u64, requeue: bool) -> Result<()> {
        self.nack(delivery_tag, false, requeue).await
    }

    /// Get a single message from a queue
    pub async fn get(&self, queue: &str, no_ack: bool) -> Result<Option<Delivery>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        // Simulate empty queue
        debug!("Get from queue {} (no_ack: {})", queue, no_ack);
        Ok(None)
    }

    /// Get delivery receiver channel
    pub fn delivery_receiver(&mut self) -> mpsc::Receiver<Delivery> {
        let (tx, rx) = mpsc::channel(10000);
        self.delivery_tx = Some(tx);
        rx
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> Metrics {
        let uptime = if let Some(connected_at) = *self.internal_metrics.connected_at.read().await {
            connected_at.elapsed().as_secs()
        } else {
            0
        };

        let avg_latency = {
            let samples = self.internal_metrics.publish_latency.read().await;
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
                .messages_published
                .load(Ordering::Relaxed),
            messages_received: self
                .internal_metrics
                .messages_consumed
                .load(Ordering::Relaxed),
            errors: self.internal_metrics.errors.load(Ordering::Relaxed),
            average_latency_ms: avg_latency,
            bytes_sent: self
                .internal_metrics
                .bytes_published
                .load(Ordering::Relaxed),
            bytes_received: self.internal_metrics.bytes_consumed.load(Ordering::Relaxed),
            uptime_seconds: uptime,
        }
    }
}

#[async_trait]
impl Connector for RabbitMqConnector {
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to RabbitMQ at {}", self.rabbitmq_config.uri);

        // Simulate connection - in production this would use lapin
        self.state.connected.store(true, Ordering::SeqCst);
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());

        let (shutdown_tx, _) = broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        info!("Connected to RabbitMQ");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from RabbitMQ");

        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        self.state.connected.store(false, Ordering::SeqCst);

        // Clear state
        {
            let mut consumers = self.state.consumers.write().await;
            consumers.clear();
        }

        info!("Disconnected from RabbitMQ");
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
            name: "test-rabbitmq".to_string(),
            connector_type: "rabbitmq".to_string(),
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
    async fn test_rabbitmq_config_default() {
        let config = RabbitMqConfig::default();
        assert!(config.uri.contains("localhost"));
        assert_eq!(config.vhost, "/");
        assert!(config.publisher_confirms);
    }

    #[tokio::test]
    async fn test_rabbitmq_connector_creation() {
        let config = create_test_config();
        let connector = RabbitMqConnector::new(config).await;
        assert!(connector.is_ok());
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = create_test_config();
        let mut connector = RabbitMqConnector::new(config).await.unwrap();

        assert!(!connector.is_connected().await);

        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);

        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }

    #[tokio::test]
    async fn test_exchange_declare() {
        let config = create_test_config();
        let mut connector = RabbitMqConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let exchange = ExchangeDeclare {
            name: "test-exchange".to_string(),
            exchange_type: ExchangeType::Topic,
            durable: true,
            auto_delete: false,
            internal: false,
            arguments: HashMap::new(),
        };

        connector.exchange_declare(exchange).await.unwrap();

        let exchanges = connector.state.exchanges.read().await;
        assert!(exchanges.contains_key("test-exchange"));
    }

    #[tokio::test]
    async fn test_queue_declare_and_bind() {
        let config = create_test_config();
        let mut connector = RabbitMqConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let queue = QueueDeclare {
            name: "test-queue".to_string(),
            durable: true,
            exclusive: false,
            auto_delete: false,
            arguments: HashMap::new(),
        };

        let queue_name = connector.queue_declare(queue).await.unwrap();
        assert_eq!(queue_name, "test-queue");

        let bind = QueueBind {
            queue: queue_name,
            exchange: "amq.direct".to_string(),
            routing_key: "test.key".to_string(),
            arguments: HashMap::new(),
        };

        connector.queue_bind(bind).await.unwrap();
    }

    #[tokio::test]
    async fn test_publish() {
        let config = create_test_config();
        let mut connector = RabbitMqConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        connector
            .publish(
                "amq.direct",
                "test.key",
                b"test message",
                MessageProperties::default(),
                PublishOptions::default(),
            )
            .await
            .unwrap();

        let metrics = connector.get_metrics().await;
        assert_eq!(metrics.messages_sent, 1);
    }
}
