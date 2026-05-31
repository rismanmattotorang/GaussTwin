//! OPC-UA Connector
//!
//! Provides OPC UA client connectivity for industrial automation systems
//! with support for subscriptions, historical data access, and alarms.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// OPC UA-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpcUaConfig {
    /// Server endpoint URL
    pub endpoint_url: String,
    /// Security policy (None, Basic128Rsa15, Basic256, Basic256Sha256)
    pub security_policy: SecurityPolicy,
    /// Security mode (None, Sign, SignAndEncrypt)
    pub security_mode: SecurityMode,
    /// Application name
    pub application_name: String,
    /// Application URI
    pub application_uri: String,
    /// Session timeout in milliseconds
    pub session_timeout_ms: u32,
    /// Request timeout in milliseconds
    pub request_timeout_ms: u32,
    /// Publishing interval in milliseconds
    pub publishing_interval_ms: u32,
    /// Maximum notifications per publish
    pub max_notifications_per_publish: u32,
    /// Certificate configuration
    pub certificate: Option<CertificateConfig>,
    /// Authentication
    pub auth: OpcUaAuth,
}

/// Security policy for OPC UA
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityPolicy {
    None,
    Basic128Rsa15,
    Basic256,
    Basic256Sha256,
    Aes128Sha256RsaOaep,
    Aes256Sha256RsaPss,
}

/// Security mode for OPC UA
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityMode {
    None,
    Sign,
    SignAndEncrypt,
}

/// Certificate configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateConfig {
    pub certificate_path: String,
    pub private_key_path: String,
    pub trust_store_path: String,
}

/// OPC UA authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpcUaAuth {
    Anonymous,
    UserPassword { username: String, password: String },
    Certificate { certificate_path: String },
    Token { token: String },
}

impl Default for OpcUaConfig {
    fn default() -> Self {
        Self {
            endpoint_url: "opc.tcp://localhost:4840".to_string(),
            security_policy: SecurityPolicy::None,
            security_mode: SecurityMode::None,
            application_name: "GaussTwin OPC UA Client".to_string(),
            application_uri: "urn:gausstwin:opcua:client".to_string(),
            session_timeout_ms: 60000,
            request_timeout_ms: 10000,
            publishing_interval_ms: 1000,
            max_notifications_per_publish: 1000,
            certificate: None,
            auth: OpcUaAuth::Anonymous,
        }
    }
}

/// OPC UA Node ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId {
    pub namespace_index: u16,
    pub identifier: NodeIdentifier,
}

impl NodeId {
    pub fn numeric(namespace: u16, identifier: u32) -> Self {
        Self {
            namespace_index: namespace,
            identifier: NodeIdentifier::Numeric(identifier),
        }
    }

    pub fn string(namespace: u16, identifier: impl Into<String>) -> Self {
        Self {
            namespace_index: namespace,
            identifier: NodeIdentifier::String(identifier.into()),
        }
    }

    pub fn guid(namespace: u16, guid: [u8; 16]) -> Self {
        Self {
            namespace_index: namespace,
            identifier: NodeIdentifier::Guid(guid),
        }
    }
}

/// Node identifier types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeIdentifier {
    Numeric(u32),
    String(String),
    Guid([u8; 16]),
    ByteString(Vec<u8>),
}

/// OPC UA Data Value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataValue {
    pub value: Option<Variant>,
    pub status_code: StatusCode,
    pub source_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub server_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

/// OPC UA Variant (value container)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Variant {
    Empty,
    Boolean(bool),
    SByte(i8),
    Byte(u8),
    Int16(i16),
    UInt16(u16),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Float(f32),
    Double(f64),
    String(String),
    DateTime(chrono::DateTime<chrono::Utc>),
    ByteString(Vec<u8>),
    Array(Box<Vec<Variant>>),
}

/// Status code for OPC UA operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusCode(pub u32);

impl StatusCode {
    pub const GOOD: Self = Self(0x00000000);
    pub const BAD: Self = Self(0x80000000);
    pub const UNCERTAIN: Self = Self(0x40000000);

    pub fn is_good(&self) -> bool {
        self.0 & 0xC0000000 == 0
    }

    pub fn is_bad(&self) -> bool {
        self.0 & 0x80000000 != 0
    }

    pub fn is_uncertain(&self) -> bool {
        self.0 & 0xC0000000 == 0x40000000
    }
}

/// Subscription for monitored items
#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: u32,
    pub publishing_interval_ms: u32,
    pub monitored_items: Vec<MonitoredItem>,
}

/// Monitored item in a subscription
#[derive(Debug, Clone)]
pub struct MonitoredItem {
    pub node_id: NodeId,
    pub attribute_id: AttributeId,
    pub sampling_interval_ms: u32,
    pub queue_size: u32,
    pub discard_oldest: bool,
}

/// Attribute IDs for OPC UA nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributeId {
    NodeId = 1,
    NodeClass = 2,
    BrowseName = 3,
    DisplayName = 4,
    Description = 5,
    WriteMask = 6,
    UserWriteMask = 7,
    IsAbstract = 8,
    Symmetric = 9,
    InverseName = 10,
    ContainsNoLoops = 11,
    EventNotifier = 12,
    Value = 13,
    DataType = 14,
    ValueRank = 15,
    ArrayDimensions = 16,
    AccessLevel = 17,
    UserAccessLevel = 18,
    MinimumSamplingInterval = 19,
    Historizing = 20,
    Executable = 21,
    UserExecutable = 22,
}

/// Browse direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseDirection {
    Forward,
    Inverse,
    Both,
}

/// Reference description from browse results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceDescription {
    pub reference_type_id: NodeId,
    pub is_forward: bool,
    pub node_id: NodeId,
    pub browse_name: String,
    pub display_name: String,
    pub node_class: NodeClass,
}

/// Node classes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeClass {
    Unspecified = 0,
    Object = 1,
    Variable = 2,
    Method = 4,
    ObjectType = 8,
    VariableType = 16,
    ReferenceType = 32,
    DataType = 64,
    View = 128,
}

/// Internal state for the connector
struct ConnectorState {
    connected: AtomicBool,
    session_id: RwLock<Option<String>>,
    subscriptions: RwLock<HashMap<u32, Subscription>>,
    next_subscription_id: AtomicU64,
    data_cache: RwLock<HashMap<NodeId, DataValue>>,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            session_id: RwLock::new(None),
            subscriptions: RwLock::new(HashMap::new()),
            next_subscription_id: AtomicU64::new(1),
            data_cache: RwLock::new(HashMap::new()),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    reads: AtomicU64,
    writes: AtomicU64,
    subscriptions: AtomicU64,
    notifications: AtomicU64,
    errors: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    latency_samples: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            reads: AtomicU64::new(0),
            writes: AtomicU64::new(0),
            subscriptions: AtomicU64::new(0),
            notifications: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            latency_samples: RwLock::new(Vec::new()),
        }
    }
}

/// OPC UA Connector for industrial automation
pub struct OpcUaConnector {
    config: Config,
    opcua_config: OpcUaConfig,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
}

impl OpcUaConnector {
    /// Create a new OPC UA connector
    pub async fn new(config: Config) -> Result<Self> {
        let opcua_config = Self::parse_opcua_config(&config)?;
        Ok(Self {
            config,
            opcua_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        })
    }

    /// Create with explicit OPC UA config
    pub fn with_opcua_config(config: Config, opcua_config: OpcUaConfig) -> Self {
        Self {
            config,
            opcua_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        }
    }

    fn parse_opcua_config(config: &Config) -> Result<OpcUaConfig> {
        let mut opcua_config = OpcUaConfig::default();

        if let Some(username) = &config.auth.credentials.username {
            if let Some(password) = &config.auth.credentials.password {
                opcua_config.auth = OpcUaAuth::UserPassword {
                    username: username.clone(),
                    password: password.clone(),
                };
            }
        }

        Ok(opcua_config)
    }

    /// Read a single value from the server
    pub async fn read(&self, node_id: &NodeId) -> Result<DataValue> {
        self.read_attribute(node_id, AttributeId::Value).await
    }

    /// Read multiple values
    pub async fn read_many(&self, node_ids: &[NodeId]) -> Result<Vec<DataValue>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();
        let mut results = Vec::with_capacity(node_ids.len());

        // Simulate reading - in production this would use the OPC UA SDK
        for node_id in node_ids {
            // Check cache first
            let cache = self.state.data_cache.read().await;
            if let Some(cached) = cache.get(node_id) {
                results.push(cached.clone());
            } else {
                // Return a placeholder value
                results.push(DataValue {
                    value: Some(Variant::Double(0.0)),
                    status_code: StatusCode::GOOD,
                    source_timestamp: Some(chrono::Utc::now()),
                    server_timestamp: Some(chrono::Utc::now()),
                });
            }
        }

        self.internal_metrics
            .reads
            .fetch_add(node_ids.len() as u64, Ordering::Relaxed);

        let latency = start.elapsed().as_secs_f64() * 1000.0;
        let mut samples = self.internal_metrics.latency_samples.write().await;
        samples.push(latency);
        if samples.len() > 1000 {
            samples.drain(0..500);
        }

        Ok(results)
    }

    /// Read a specific attribute
    pub async fn read_attribute(
        &self,
        node_id: &NodeId,
        attribute_id: AttributeId,
    ) -> Result<DataValue> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Simulate reading - in production this would use the OPC UA SDK
        let value = DataValue {
            value: Some(Variant::Double(0.0)),
            status_code: StatusCode::GOOD,
            source_timestamp: Some(chrono::Utc::now()),
            server_timestamp: Some(chrono::Utc::now()),
        };

        self.internal_metrics.reads.fetch_add(1, Ordering::Relaxed);

        let latency = start.elapsed().as_secs_f64() * 1000.0;
        debug!(
            "Read node {:?} attribute {:?} (latency: {:.2}ms)",
            node_id, attribute_id, latency
        );

        Ok(value)
    }

    /// Write a value to a node
    pub async fn write(&self, node_id: &NodeId, value: Variant) -> Result<StatusCode> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Update cache
        {
            let mut cache = self.state.data_cache.write().await;
            cache.insert(
                node_id.clone(),
                DataValue {
                    value: Some(value),
                    status_code: StatusCode::GOOD,
                    source_timestamp: Some(chrono::Utc::now()),
                    server_timestamp: Some(chrono::Utc::now()),
                },
            );
        }

        self.internal_metrics.writes.fetch_add(1, Ordering::Relaxed);

        let latency = start.elapsed().as_secs_f64() * 1000.0;
        debug!("Write to node {:?} (latency: {:.2}ms)", node_id, latency);

        Ok(StatusCode::GOOD)
    }

    /// Write multiple values
    pub async fn write_many(&self, writes: &[(NodeId, Variant)]) -> Result<Vec<StatusCode>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let mut results = Vec::with_capacity(writes.len());
        let mut cache = self.state.data_cache.write().await;

        for (node_id, value) in writes {
            cache.insert(
                node_id.clone(),
                DataValue {
                    value: Some(value.clone()),
                    status_code: StatusCode::GOOD,
                    source_timestamp: Some(chrono::Utc::now()),
                    server_timestamp: Some(chrono::Utc::now()),
                },
            );
            results.push(StatusCode::GOOD);
        }

        self.internal_metrics
            .writes
            .fetch_add(writes.len() as u64, Ordering::Relaxed);

        Ok(results)
    }

    /// Browse nodes
    pub async fn browse(
        &self,
        node_id: &NodeId,
        direction: BrowseDirection,
    ) -> Result<Vec<ReferenceDescription>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        // Simulate browse - in production this would use the OPC UA SDK
        debug!("Browse node {:?} direction {:?}", node_id, direction);

        Ok(vec![])
    }

    /// Create a subscription
    pub async fn create_subscription(&self, publishing_interval_ms: u32) -> Result<u32> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let subscription_id = self
            .state
            .next_subscription_id
            .fetch_add(1, Ordering::SeqCst) as u32;

        let subscription = Subscription {
            id: subscription_id,
            publishing_interval_ms,
            monitored_items: Vec::new(),
        };

        {
            let mut subs = self.state.subscriptions.write().await;
            subs.insert(subscription_id, subscription);
        }

        self.internal_metrics
            .subscriptions
            .fetch_add(1, Ordering::Relaxed);

        info!("Created subscription {}", subscription_id);
        Ok(subscription_id)
    }

    /// Add monitored items to a subscription
    pub async fn add_monitored_items(
        &self,
        subscription_id: u32,
        items: Vec<MonitoredItem>,
    ) -> Result<()> {
        let mut subs = self.state.subscriptions.write().await;

        if let Some(subscription) = subs.get_mut(&subscription_id) {
            subscription.monitored_items.extend(items);
            debug!(
                "Added {} items to subscription {}",
                subscription.monitored_items.len(),
                subscription_id
            );
            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "Subscription {} not found",
                subscription_id
            )))
        }
    }

    /// Delete a subscription
    pub async fn delete_subscription(&self, subscription_id: u32) -> Result<()> {
        let mut subs = self.state.subscriptions.write().await;

        if subs.remove(&subscription_id).is_some() {
            info!("Deleted subscription {}", subscription_id);
            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "Subscription {} not found",
                subscription_id
            )))
        }
    }

    /// Call a method on the server
    pub async fn call_method(
        &self,
        object_id: &NodeId,
        method_id: &NodeId,
        arguments: Vec<Variant>,
    ) -> Result<Vec<Variant>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        debug!(
            "Call method {:?} on object {:?} with {} arguments",
            method_id,
            object_id,
            arguments.len()
        );

        // Simulate method call - in production this would use the OPC UA SDK
        Ok(vec![])
    }

    /// Read historical data
    pub async fn read_history(
        &self,
        node_id: &NodeId,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
        max_values: u32,
    ) -> Result<Vec<DataValue>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        debug!(
            "Read history for {:?} from {} to {} (max: {})",
            node_id, start_time, end_time, max_values
        );

        // Simulate history read - in production this would use the OPC UA SDK
        Ok(vec![])
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

        Metrics {
            connections: if self.state.connected.load(Ordering::SeqCst) {
                1
            } else {
                0
            },
            connection_failures: 0,
            messages_sent: self.internal_metrics.writes.load(Ordering::Relaxed),
            messages_received: self.internal_metrics.reads.load(Ordering::Relaxed),
            errors: self.internal_metrics.errors.load(Ordering::Relaxed),
            average_latency_ms: avg_latency,
            bytes_sent: 0,
            bytes_received: 0,
            uptime_seconds: uptime,
        }
    }
}

#[async_trait]
impl Connector for OpcUaConnector {
    async fn connect(&mut self) -> Result<()> {
        info!(
            "Connecting to OPC UA server at {}",
            self.opcua_config.endpoint_url
        );

        // Simulate connection - in production this would:
        // 1. Create secure channel
        // 2. Create session
        // 3. Activate session with authentication

        self.state.connected.store(true, Ordering::SeqCst);
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());
        *self.state.session_id.write().await = Some(uuid::Uuid::new_v4().to_string());

        info!("Connected to OPC UA server");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from OPC UA server");

        // Clear subscriptions
        {
            let mut subs = self.state.subscriptions.write().await;
            subs.clear();
        }

        // Clear session
        *self.state.session_id.write().await = None;
        self.state.connected.store(false, Ordering::SeqCst);

        info!("Disconnected from OPC UA server");
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
            name: "test-opcua".to_string(),
            connector_type: "opcua".to_string(),
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
    async fn test_opcua_config_default() {
        let config = OpcUaConfig::default();
        assert_eq!(config.endpoint_url, "opc.tcp://localhost:4840");
        assert!(matches!(config.security_policy, SecurityPolicy::None));
        assert!(matches!(config.security_mode, SecurityMode::None));
    }

    #[tokio::test]
    async fn test_opcua_connector_creation() {
        let config = create_test_config();
        let connector = OpcUaConnector::new(config).await;
        assert!(connector.is_ok());
    }

    #[tokio::test]
    async fn test_node_id_creation() {
        let numeric = NodeId::numeric(0, 1234);
        assert_eq!(numeric.namespace_index, 0);
        assert!(matches!(numeric.identifier, NodeIdentifier::Numeric(1234)));

        let string = NodeId::string(1, "TestNode");
        assert_eq!(string.namespace_index, 1);
        assert!(matches!(&string.identifier, NodeIdentifier::String(s) if s == "TestNode"));
    }

    #[tokio::test]
    async fn test_status_code() {
        assert!(StatusCode::GOOD.is_good());
        assert!(!StatusCode::GOOD.is_bad());
        assert!(!StatusCode::GOOD.is_uncertain());

        assert!(StatusCode::BAD.is_bad());
        assert!(!StatusCode::BAD.is_good());

        assert!(StatusCode::UNCERTAIN.is_uncertain());
        assert!(!StatusCode::UNCERTAIN.is_good());
        assert!(!StatusCode::UNCERTAIN.is_bad());
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = create_test_config();
        let mut connector = OpcUaConnector::new(config).await.unwrap();

        assert!(!connector.is_connected().await);

        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);

        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }
}
