//! BACnet Connector
//!
//! Provides connectivity to BACnet building automation systems
//! supporting BACnet/IP protocol for reading/writing object properties.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// BACnet-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacnetConfig {
    /// Local IP address
    pub local_address: String,
    /// BACnet/IP port (default 47808)
    pub port: u16,
    /// BBMD address (for NAT traversal)
    pub bbmd_address: Option<String>,
    /// Device instance number
    pub device_instance: u32,
    /// Maximum APDU length
    pub max_apdu_length: u16,
    /// Segmentation support
    pub segmentation: SegmentationSupport,
    /// Timeout in milliseconds
    pub timeout_ms: u64,
    /// Retry count
    pub retry_count: u32,
    /// APDU retries
    pub apdu_retries: u32,
    /// APDU timeout in milliseconds
    pub apdu_timeout_ms: u64,
}

/// Segmentation support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentationSupport {
    NoSegmentation,
    SegmentedTransmit,
    SegmentedReceive,
    SegmentedBoth,
}

impl Default for BacnetConfig {
    fn default() -> Self {
        Self {
            local_address: "0.0.0.0".to_string(),
            port: 47808,
            bbmd_address: None,
            device_instance: 1234,
            max_apdu_length: 1476,
            segmentation: SegmentationSupport::SegmentedBoth,
            timeout_ms: 5000,
            retry_count: 3,
            apdu_retries: 3,
            apdu_timeout_ms: 3000,
        }
    }
}

/// BACnet object types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectType {
    AnalogInput,
    AnalogOutput,
    AnalogValue,
    BinaryInput,
    BinaryOutput,
    BinaryValue,
    Calendar,
    Command,
    Device,
    EventEnrollment,
    File,
    Group,
    Loop,
    MultiStateInput,
    MultiStateOutput,
    NotificationClass,
    Program,
    Schedule,
    Averaging,
    MultiStateValue,
    TrendLog,
    LifeSafetyPoint,
    LifeSafetyZone,
    Accumulator,
    PulseConverter,
    EventLog,
    TrendLogMultiple,
    LoadControl,
    StructuredView,
    AccessDoor,
    AccessCredential,
    AccessPoint,
    AccessRights,
    AccessUser,
    AccessZone,
    CredentialDataInput,
    NetworkPort,
    ElevatorGroup,
    Escalator,
    Lift,
}

impl ObjectType {
    pub fn code(&self) -> u16 {
        match self {
            ObjectType::AnalogInput => 0,
            ObjectType::AnalogOutput => 1,
            ObjectType::AnalogValue => 2,
            ObjectType::BinaryInput => 3,
            ObjectType::BinaryOutput => 4,
            ObjectType::BinaryValue => 5,
            ObjectType::Calendar => 6,
            ObjectType::Command => 7,
            ObjectType::Device => 8,
            ObjectType::EventEnrollment => 9,
            ObjectType::File => 10,
            ObjectType::Group => 11,
            ObjectType::Loop => 12,
            ObjectType::MultiStateInput => 13,
            ObjectType::MultiStateOutput => 14,
            ObjectType::NotificationClass => 15,
            ObjectType::Program => 16,
            ObjectType::Schedule => 17,
            ObjectType::Averaging => 18,
            ObjectType::MultiStateValue => 19,
            ObjectType::TrendLog => 20,
            ObjectType::LifeSafetyPoint => 21,
            ObjectType::LifeSafetyZone => 22,
            ObjectType::Accumulator => 23,
            ObjectType::PulseConverter => 24,
            ObjectType::EventLog => 25,
            ObjectType::TrendLogMultiple => 27,
            ObjectType::LoadControl => 28,
            ObjectType::StructuredView => 29,
            ObjectType::AccessDoor => 30,
            ObjectType::AccessCredential => 32,
            ObjectType::AccessPoint => 33,
            ObjectType::AccessRights => 34,
            ObjectType::AccessUser => 35,
            ObjectType::AccessZone => 36,
            ObjectType::CredentialDataInput => 37,
            ObjectType::NetworkPort => 56,
            ObjectType::ElevatorGroup => 57,
            ObjectType::Escalator => 58,
            ObjectType::Lift => 59,
        }
    }
}

/// BACnet property identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PropertyId {
    PresentValue,
    ObjectName,
    ObjectType,
    SystemStatus,
    VendorName,
    VendorIdentifier,
    ModelName,
    FirmwareRevision,
    ApplicationSoftwareVersion,
    Description,
    Location,
    StatusFlags,
    EventState,
    Reliability,
    OutOfService,
    Units,
    MinPresValue,
    MaxPresValue,
    Resolution,
    PolariTy,
    StateText,
    NumberOfStates,
    Relinquishdefault,
    PriorityArray,
    CovIncrement,
    TimeDelay,
    NotificationClass,
    LimitEnable,
    EventEnable,
    AckedTransitions,
    NotifyType,
    ActiveText,
    InactiveText,
    AlarmValue,
}

impl PropertyId {
    pub fn code(&self) -> u32 {
        match self {
            PropertyId::PresentValue => 85,
            PropertyId::ObjectName => 77,
            PropertyId::ObjectType => 79,
            PropertyId::SystemStatus => 112,
            PropertyId::VendorName => 121,
            PropertyId::VendorIdentifier => 120,
            PropertyId::ModelName => 70,
            PropertyId::FirmwareRevision => 44,
            PropertyId::ApplicationSoftwareVersion => 12,
            PropertyId::Description => 28,
            PropertyId::Location => 58,
            PropertyId::StatusFlags => 111,
            PropertyId::EventState => 36,
            PropertyId::Reliability => 103,
            PropertyId::OutOfService => 81,
            PropertyId::Units => 117,
            PropertyId::MinPresValue => 69,
            PropertyId::MaxPresValue => 65,
            PropertyId::Resolution => 106,
            PropertyId::PolariTy => 84,
            PropertyId::StateText => 110,
            PropertyId::NumberOfStates => 74,
            PropertyId::Relinquishdefault => 104,
            PropertyId::PriorityArray => 87,
            PropertyId::CovIncrement => 22,
            PropertyId::TimeDelay => 113,
            PropertyId::NotificationClass => 17,
            PropertyId::LimitEnable => 52,
            PropertyId::EventEnable => 35,
            PropertyId::AckedTransitions => 0,
            PropertyId::NotifyType => 72,
            PropertyId::ActiveText => 4,
            PropertyId::InactiveText => 46,
            PropertyId::AlarmValue => 6,
        }
    }
}

/// BACnet object identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId {
    pub object_type: ObjectType,
    pub instance: u32,
}

impl ObjectId {
    pub fn new(object_type: ObjectType, instance: u32) -> Self {
        Self {
            object_type,
            instance,
        }
    }

    pub fn analog_input(instance: u32) -> Self {
        Self::new(ObjectType::AnalogInput, instance)
    }

    pub fn analog_output(instance: u32) -> Self {
        Self::new(ObjectType::AnalogOutput, instance)
    }

    pub fn binary_input(instance: u32) -> Self {
        Self::new(ObjectType::BinaryInput, instance)
    }

    pub fn binary_output(instance: u32) -> Self {
        Self::new(ObjectType::BinaryOutput, instance)
    }
}

/// BACnet value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BacnetValue {
    Null,
    Boolean(bool),
    UnsignedInt(u64),
    SignedInt(i64),
    Real(f32),
    Double(f64),
    OctetString(Vec<u8>),
    CharacterString(String),
    BitString(Vec<bool>),
    Enumerated(u32),
    Date(BacnetDate),
    Time(BacnetTime),
    ObjectId(ObjectId),
    Array(Vec<BacnetValue>),
}

/// BACnet date
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BacnetDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub day_of_week: u8,
}

/// BACnet time
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BacnetTime {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub hundredths: u8,
}

/// Read property result
#[derive(Debug, Clone)]
pub struct ReadPropertyResult {
    pub object_id: ObjectId,
    pub property_id: PropertyId,
    pub value: BacnetValue,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Device info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: u32,
    pub vendor_name: String,
    pub vendor_id: u16,
    pub model_name: String,
    pub firmware_revision: String,
    pub application_software_version: String,
    pub object_name: String,
    pub description: String,
    pub location: String,
}

/// Internal state
struct ConnectorState {
    connected: AtomicBool,
    objects: RwLock<HashMap<ObjectId, HashMap<PropertyId, BacnetValue>>>,
    devices: RwLock<HashMap<u32, DeviceInfo>>,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            objects: RwLock::new(HashMap::new()),
            devices: RwLock::new(HashMap::new()),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    reads: AtomicU64,
    writes: AtomicU64,
    who_is_count: AtomicU64,
    cov_notifications: AtomicU64,
    errors: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    latency_samples: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            reads: AtomicU64::new(0),
            writes: AtomicU64::new(0),
            who_is_count: AtomicU64::new(0),
            cov_notifications: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            latency_samples: RwLock::new(Vec::new()),
        }
    }
}

/// BACnet Connector
pub struct BacnetConnector {
    config: Config,
    bacnet_config: BacnetConfig,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
}

impl BacnetConnector {
    /// Create a new BACnet connector
    pub async fn new(config: Config) -> Result<Self> {
        let bacnet_config = BacnetConfig::default();
        Ok(Self {
            config,
            bacnet_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        })
    }

    /// Create with explicit BACnet config
    pub fn with_bacnet_config(config: Config, bacnet_config: BacnetConfig) -> Self {
        Self {
            config,
            bacnet_config,
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

    /// Read a property
    pub async fn read_property(
        &self,
        device_id: u32,
        object_id: ObjectId,
        property_id: PropertyId,
    ) -> Result<ReadPropertyResult> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        let value = {
            let objects = self.state.objects.read().await;
            objects
                .get(&object_id)
                .and_then(|props| props.get(&property_id))
                .cloned()
                .unwrap_or(BacnetValue::Null)
        };

        self.internal_metrics.reads.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(ReadPropertyResult {
            object_id,
            property_id,
            value,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Read multiple properties
    pub async fn read_property_multiple(
        &self,
        device_id: u32,
        requests: &[(ObjectId, Vec<PropertyId>)],
    ) -> Result<Vec<ReadPropertyResult>> {
        let mut results = Vec::new();

        for (object_id, properties) in requests {
            for property_id in properties {
                results.push(
                    self.read_property(device_id, *object_id, *property_id)
                        .await?,
                );
            }
        }

        Ok(results)
    }

    /// Write a property
    pub async fn write_property(
        &self,
        device_id: u32,
        object_id: ObjectId,
        property_id: PropertyId,
        value: BacnetValue,
        priority: Option<u8>,
    ) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        {
            let mut objects = self.state.objects.write().await;
            let props = objects.entry(object_id).or_insert_with(HashMap::new);
            props.insert(property_id, value);
        }

        self.internal_metrics.writes.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!(
            "Write property {:?}:{:?} on device {}",
            object_id, property_id, device_id
        );
        Ok(())
    }

    /// Discover devices (Who-Is)
    pub async fn who_is(
        &self,
        low_limit: Option<u32>,
        high_limit: Option<u32>,
    ) -> Result<Vec<DeviceInfo>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Return simulated devices
        let devices = {
            let devices = self.state.devices.read().await;
            devices
                .values()
                .filter(|d| {
                    let low = low_limit.unwrap_or(0);
                    let high = high_limit.unwrap_or(u32::MAX);
                    d.device_id >= low && d.device_id <= high
                })
                .cloned()
                .collect()
        };

        self.internal_metrics
            .who_is_count
            .fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(devices)
    }

    /// Subscribe to COV (Change of Value) notifications
    pub async fn subscribe_cov(
        &self,
        device_id: u32,
        object_id: ObjectId,
        confirmed: bool,
        lifetime: Option<u32>,
    ) -> Result<u32> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let subscription_id = uuid::Uuid::new_v4().as_u128() as u32;

        debug!(
            "Subscribed to COV for {:?} on device {} (sub_id: {})",
            object_id, device_id, subscription_id
        );

        Ok(subscription_id)
    }

    /// Unsubscribe from COV notifications
    pub async fn unsubscribe_cov(
        &self,
        device_id: u32,
        object_id: ObjectId,
        subscription_id: u32,
    ) -> Result<()> {
        debug!(
            "Unsubscribed from COV for {:?} on device {} (sub_id: {})",
            object_id, device_id, subscription_id
        );
        Ok(())
    }

    /// Create a simulated device
    pub async fn create_simulated_device(&self, device_info: DeviceInfo) -> Result<()> {
        let mut devices = self.state.devices.write().await;
        devices.insert(device_info.device_id, device_info);
        Ok(())
    }

    /// Create a simulated object
    pub async fn create_simulated_object(
        &self,
        object_id: ObjectId,
        properties: HashMap<PropertyId, BacnetValue>,
    ) -> Result<()> {
        let mut objects = self.state.objects.write().await;
        objects.insert(object_id, properties);
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
impl Connector for BacnetConnector {
    async fn connect(&mut self) -> Result<()> {
        info!(
            "Connecting to BACnet network on {}:{}",
            self.bacnet_config.local_address, self.bacnet_config.port
        );

        self.state.connected.store(true, Ordering::SeqCst);
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());

        // Create default simulated device
        let device_info = DeviceInfo {
            device_id: self.bacnet_config.device_instance,
            vendor_name: "GaussTwin".to_string(),
            vendor_id: 999,
            model_name: "BACnet Simulator".to_string(),
            firmware_revision: "1.0.0".to_string(),
            application_software_version: "1.0.0".to_string(),
            object_name: "GaussTwin BACnet Device".to_string(),
            description: "Simulated BACnet device for testing".to_string(),
            location: "Virtual".to_string(),
        };
        self.create_simulated_device(device_info).await?;

        // Create some default objects
        let mut ai_props = HashMap::new();
        ai_props.insert(PropertyId::PresentValue, BacnetValue::Real(21.5));
        ai_props.insert(
            PropertyId::ObjectName,
            BacnetValue::CharacterString("Temperature".to_string()),
        );
        ai_props.insert(PropertyId::Units, BacnetValue::Enumerated(62)); // degrees-celsius
        self.create_simulated_object(ObjectId::analog_input(0), ai_props)
            .await?;

        info!("Connected to BACnet network");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from BACnet network");
        self.state.connected.store(false, Ordering::SeqCst);
        info!("Disconnected from BACnet network");
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
            name: "test-bacnet".to_string(),
            connector_type: "bacnet".to_string(),
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
    async fn test_bacnet_config_default() {
        let config = BacnetConfig::default();
        assert_eq!(config.port, 47808);
        assert_eq!(config.device_instance, 1234);
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = create_test_config();
        let mut connector = BacnetConnector::new(config).await.unwrap();

        assert!(!connector.is_connected().await);
        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);
        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }

    #[tokio::test]
    async fn test_read_write_property() {
        let config = create_test_config();
        let mut connector = BacnetConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let object_id = ObjectId::analog_input(0);

        // Read default value
        let result = connector
            .read_property(1234, object_id, PropertyId::PresentValue)
            .await
            .unwrap();

        match result.value {
            BacnetValue::Real(v) => assert!((v - 21.5).abs() < 0.01),
            _ => panic!("Unexpected value type"),
        }

        // Write new value
        connector
            .write_property(
                1234,
                object_id,
                PropertyId::PresentValue,
                BacnetValue::Real(25.0),
                None,
            )
            .await
            .unwrap();

        // Read back
        let result = connector
            .read_property(1234, object_id, PropertyId::PresentValue)
            .await
            .unwrap();

        match result.value {
            BacnetValue::Real(v) => assert!((v - 25.0).abs() < 0.01),
            _ => panic!("Unexpected value type"),
        }
    }

    #[tokio::test]
    async fn test_who_is() {
        let config = create_test_config();
        let mut connector = BacnetConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let devices = connector.who_is(None, None).await.unwrap();
        assert!(!devices.is_empty());
    }
}
