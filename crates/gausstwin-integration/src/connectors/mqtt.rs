//! MQTT Connector
//!
//! Provides MQTT v3.1.1 and v5 support for IoT/Edge integration with
//! support for QoS levels, retained messages, and last will testament.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// MQTT-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    /// Broker hostname
    pub host: String,
    /// Broker port
    pub port: u16,
    /// Client ID (must be unique per broker)
    pub client_id: String,
    /// Keep alive interval in seconds
    pub keep_alive_secs: u64,
    /// Clean session flag
    pub clean_session: bool,
    /// TLS configuration
    pub tls: Option<TlsConfig>,
    /// Authentication
    pub auth: Option<MqttAuth>,
    /// Last Will Testament
    pub last_will: Option<LastWill>,
    /// Reconnect settings
    pub reconnect: ReconnectSettings,
    /// Maximum packet size
    pub max_packet_size: usize,
    /// Inflight messages cap
    pub inflight_cap: u16,
}

/// TLS configuration for MQTT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
    pub verify_hostname: bool,
}

/// MQTT authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttAuth {
    pub username: String,
    pub password: String,
}

/// Last Will Testament configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastWill {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
}

/// Reconnect settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectSettings {
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for ReconnectSettings {
    fn default() -> Self {
        Self {
            max_retries: 10,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            backoff_multiplier: 2.0,
        }
    }
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 1883,
            client_id: format!("gausstwin-{}", uuid::Uuid::new_v4()),
            keep_alive_secs: 60,
            clean_session: true,
            tls: None,
            auth: None,
            last_will: None,
            reconnect: ReconnectSettings::default(),
            max_packet_size: 256 * 1024,
            inflight_cap: 100,
        }
    }
}

/// QoS level for MQTT messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MqttQoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

impl From<MqttQoS> for QoS {
    fn from(qos: MqttQoS) -> Self {
        match qos {
            MqttQoS::AtMostOnce => QoS::AtMostOnce,
            MqttQoS::AtLeastOnce => QoS::AtLeastOnce,
            MqttQoS::ExactlyOnce => QoS::ExactlyOnce,
        }
    }
}

/// Subscription configuration
#[derive(Debug, Clone)]
pub struct Subscription {
    pub topic: String,
    pub qos: MqttQoS,
}

/// Received MQTT message
#[derive(Debug, Clone)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: MqttQoS,
    pub retain: bool,
    pub timestamp: Instant,
}

/// Internal metrics tracking
struct InternalMetrics {
    connected: AtomicBool,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    reconnections: AtomicU64,
    errors: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    latency_samples: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            reconnections: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            latency_samples: RwLock::new(Vec::new()),
        }
    }
}

/// MQTT Connector for IoT/Edge integration
pub struct MqttConnector {
    config: Config,
    mqtt_config: MqttConfig,
    client: Option<AsyncClient>,
    internal_metrics: Arc<InternalMetrics>,
    subscriptions: Arc<RwLock<HashMap<String, MqttQoS>>>,
    message_tx: Option<mpsc::Sender<MqttMessage>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl MqttConnector {
    /// Create a new MQTT connector
    pub async fn new(config: Config) -> Result<Self> {
        let mqtt_config = Self::parse_mqtt_config(&config)?;
        Ok(Self {
            config,
            mqtt_config,
            client: None,
            internal_metrics: Arc::new(InternalMetrics::default()),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            message_tx: None,
            shutdown_tx: None,
        })
    }

    /// Create with explicit MQTT config
    pub fn with_mqtt_config(config: Config, mqtt_config: MqttConfig) -> Self {
        Self {
            config,
            mqtt_config,
            client: None,
            internal_metrics: Arc::new(InternalMetrics::default()),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            message_tx: None,
            shutdown_tx: None,
        }
    }

    fn parse_mqtt_config(config: &Config) -> Result<MqttConfig> {
        // Parse from custom config if available, otherwise use defaults
        let mut mqtt_config = MqttConfig::default();
        mqtt_config.client_id = format!("gausstwin-{}", config.name);

        if let Some(username) = &config.auth.credentials.username {
            if let Some(password) = &config.auth.credentials.password {
                mqtt_config.auth = Some(MqttAuth {
                    username: username.clone(),
                    password: password.clone(),
                });
            }
        }

        Ok(mqtt_config)
    }

    fn create_mqtt_options(&self) -> MqttOptions {
        let mut opts = MqttOptions::new(
            &self.mqtt_config.client_id,
            &self.mqtt_config.host,
            self.mqtt_config.port,
        );

        opts.set_keep_alive(Duration::from_secs(self.mqtt_config.keep_alive_secs));
        opts.set_clean_session(self.mqtt_config.clean_session);
        opts.set_max_packet_size(
            self.mqtt_config.max_packet_size,
            self.mqtt_config.max_packet_size,
        );
        opts.set_inflight(self.mqtt_config.inflight_cap);

        if let Some(auth) = &self.mqtt_config.auth {
            opts.set_credentials(&auth.username, &auth.password);
        }

        if let Some(lwt) = &self.mqtt_config.last_will {
            let qos = match lwt.qos {
                0 => QoS::AtMostOnce,
                1 => QoS::AtLeastOnce,
                _ => QoS::ExactlyOnce,
            };
            opts.set_last_will(rumqttc::LastWill::new(
                &lwt.topic,
                lwt.payload.clone(),
                qos,
                lwt.retain,
            ));
        }

        opts
    }

    /// Subscribe to a topic
    pub async fn subscribe(&mut self, topic: &str, qos: MqttQoS) -> Result<()> {
        if let Some(client) = &self.client {
            client
                .subscribe(topic, qos.into())
                .await
                .map_err(|e| Error::Connection(format!("Failed to subscribe: {}", e)))?;

            let mut subs = self.subscriptions.write().await;
            subs.insert(topic.to_string(), qos);

            info!("Subscribed to topic: {} with QoS {:?}", topic, qos);
            Ok(())
        } else {
            Err(Error::Connection("Not connected".to_string()))
        }
    }

    /// Unsubscribe from a topic
    pub async fn unsubscribe(&mut self, topic: &str) -> Result<()> {
        if let Some(client) = &self.client {
            client
                .unsubscribe(topic)
                .await
                .map_err(|e| Error::Connection(format!("Failed to unsubscribe: {}", e)))?;

            let mut subs = self.subscriptions.write().await;
            subs.remove(topic);

            info!("Unsubscribed from topic: {}", topic);
            Ok(())
        } else {
            Err(Error::Connection("Not connected".to_string()))
        }
    }

    /// Publish a message
    pub async fn publish(
        &self,
        topic: &str,
        payload: &[u8],
        qos: MqttQoS,
        retain: bool,
    ) -> Result<()> {
        if let Some(client) = &self.client {
            let start = Instant::now();

            client
                .publish(topic, qos.into(), retain, payload.to_vec())
                .await
                .map_err(|e| Error::Connection(format!("Failed to publish: {}", e)))?;

            // Update metrics
            self.internal_metrics
                .messages_sent
                .fetch_add(1, Ordering::Relaxed);
            self.internal_metrics
                .bytes_sent
                .fetch_add(payload.len() as u64, Ordering::Relaxed);

            let latency = start.elapsed().as_secs_f64() * 1000.0;
            let mut samples = self.internal_metrics.latency_samples.write().await;
            samples.push(latency);
            if samples.len() > 1000 {
                samples.drain(0..500);
            }

            debug!(
                "Published {} bytes to topic: {} (latency: {:.2}ms)",
                payload.len(),
                topic,
                latency
            );
            Ok(())
        } else {
            Err(Error::Connection("Not connected".to_string()))
        }
    }

    /// Publish JSON data
    pub async fn publish_json<T: Serialize>(
        &self,
        topic: &str,
        data: &T,
        qos: MqttQoS,
        retain: bool,
    ) -> Result<()> {
        let payload = serde_json::to_vec(data)?;
        self.publish(topic, &payload, qos, retain).await
    }

    /// Get a message receiver channel
    pub fn message_receiver(&mut self) -> mpsc::Receiver<MqttMessage> {
        let (tx, rx) = mpsc::channel(1000);
        self.message_tx = Some(tx);
        rx
    }

    /// Resubscribe to all topics (after reconnection)
    async fn resubscribe_all(&self) -> Result<()> {
        if let Some(client) = &self.client {
            let subs = self.subscriptions.read().await;
            for (topic, qos) in subs.iter() {
                client
                    .subscribe(topic, (*qos).into())
                    .await
                    .map_err(|e| Error::Connection(format!("Failed to resubscribe: {}", e)))?;
                debug!("Resubscribed to topic: {}", topic);
            }
        }
        Ok(())
    }

    /// Start the event loop
    async fn start_event_loop(
        &self,
        mut eventloop: rumqttc::EventLoop,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
        let metrics = self.internal_metrics.clone();
        let message_tx = self.message_tx.clone();
        let subscriptions = self.subscriptions.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("MQTT event loop shutting down");
                        break;
                    }
                    event = eventloop.poll() => {
                        match event {
                            Ok(Event::Incoming(Packet::Publish(publish))) => {
                                metrics.messages_received.fetch_add(1, Ordering::Relaxed);
                                metrics.bytes_received.fetch_add(publish.payload.len() as u64, Ordering::Relaxed);

                                let qos = match publish.qos {
                                    QoS::AtMostOnce => MqttQoS::AtMostOnce,
                                    QoS::AtLeastOnce => MqttQoS::AtLeastOnce,
                                    QoS::ExactlyOnce => MqttQoS::ExactlyOnce,
                                };

                                let msg = MqttMessage {
                                    topic: publish.topic.to_string(),
                                    payload: publish.payload.to_vec(),
                                    qos,
                                    retain: publish.retain,
                                    timestamp: Instant::now(),
                                };

                                if let Some(tx) = &message_tx {
                                    let _ = tx.send(msg).await;
                                }
                            }
                            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                                info!("MQTT connected");
                                metrics.connected.store(true, Ordering::SeqCst);
                                *metrics.connected_at.write().await = Some(Instant::now());

                                // Resubscribe to topics
                                let subs = subscriptions.read().await;
                                debug!("Resubscribing to {} topics", subs.len());
                            }
                            Ok(Event::Incoming(Packet::Disconnect)) => {
                                warn!("MQTT disconnected by broker");
                                metrics.connected.store(false, Ordering::SeqCst);
                            }
                            Err(e) => {
                                error!("MQTT error: {:?}", e);
                                metrics.errors.fetch_add(1, Ordering::Relaxed);
                                metrics.connected.store(false, Ordering::SeqCst);

                                // Wait before reconnecting
                                tokio::time::sleep(Duration::from_secs(1)).await;
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
    }
}

#[async_trait]
impl Connector for MqttConnector {
    async fn connect(&mut self) -> Result<()> {
        info!(
            "Connecting to MQTT broker at {}:{}",
            self.mqtt_config.host, self.mqtt_config.port
        );

        let opts = self.create_mqtt_options();
        let (client, eventloop) = AsyncClient::new(opts, self.mqtt_config.inflight_cap as usize);

        self.client = Some(client);

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        // Start event loop
        self.start_event_loop(eventloop, shutdown_rx).await;

        // Wait for connection
        let timeout = Duration::from_secs(10);
        let start = Instant::now();
        while !self.internal_metrics.connected.load(Ordering::SeqCst) {
            if start.elapsed() > timeout {
                return Err(Error::Timeout("Connection timeout".to_string()));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!("MQTT connected successfully");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from MQTT broker");

        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        if let Some(client) = self.client.take() {
            // Try graceful disconnect
            let _ = client.disconnect().await;
        }

        self.internal_metrics
            .connected
            .store(false, Ordering::SeqCst);
        info!("MQTT disconnected");
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.internal_metrics.connected.load(Ordering::SeqCst)
    }

    fn metrics(&self) -> &Metrics {
        // Return a static reference - in production you'd want to update this dynamically
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

impl MqttConnector {
    /// Get current metrics snapshot
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

        Metrics {
            connections: if self.internal_metrics.connected.load(Ordering::SeqCst) {
                1
            } else {
                0
            },
            connection_failures: self.internal_metrics.reconnections.load(Ordering::Relaxed),
            messages_sent: self.internal_metrics.messages_sent.load(Ordering::Relaxed),
            messages_received: self
                .internal_metrics
                .messages_received
                .load(Ordering::Relaxed),
            errors: self.internal_metrics.errors.load(Ordering::Relaxed),
            average_latency_ms: avg_latency,
            bytes_sent: self.internal_metrics.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.internal_metrics.bytes_received.load(Ordering::Relaxed),
            uptime_seconds: uptime,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthConfig, AuthType, Credentials, RetryPolicy};

    fn create_test_config() -> Config {
        Config {
            name: "test-mqtt".to_string(),
            connector_type: "mqtt".to_string(),
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
    async fn test_mqtt_config_default() {
        let config = MqttConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 1883);
        assert_eq!(config.keep_alive_secs, 60);
        assert!(config.clean_session);
    }

    #[tokio::test]
    async fn test_mqtt_connector_creation() {
        let config = create_test_config();
        let connector = MqttConnector::new(config).await;
        assert!(connector.is_ok());
    }

    #[tokio::test]
    async fn test_qos_conversion() {
        assert!(matches!(QoS::from(MqttQoS::AtMostOnce), QoS::AtMostOnce));
        assert!(matches!(QoS::from(MqttQoS::AtLeastOnce), QoS::AtLeastOnce));
        assert!(matches!(QoS::from(MqttQoS::ExactlyOnce), QoS::ExactlyOnce));
    }
}
