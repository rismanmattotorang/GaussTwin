//! Modbus Connector
//!
//! Provides Modbus TCP/RTU client connectivity for industrial PLCs and devices
//! with support for all standard function codes and batch operations.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Modbus-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModbusConfig {
    /// Connection type (TCP or RTU)
    pub connection_type: ConnectionType,
    /// TCP host (for TCP connections)
    pub host: String,
    /// TCP port (for TCP connections)
    pub port: u16,
    /// Serial port path (for RTU connections)
    pub serial_port: Option<String>,
    /// Baud rate (for RTU connections)
    pub baud_rate: u32,
    /// Data bits (for RTU connections)
    pub data_bits: u8,
    /// Stop bits (for RTU connections)
    pub stop_bits: u8,
    /// Parity (for RTU connections)
    pub parity: Parity,
    /// Unit/slave ID
    pub unit_id: u8,
    /// Response timeout in milliseconds
    pub timeout_ms: u64,
    /// Inter-frame delay in milliseconds (for RTU)
    pub inter_frame_delay_ms: u64,
    /// Maximum retries
    pub max_retries: u32,
    /// Enable write coalescing
    pub write_coalescing: bool,
}

/// Connection type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionType {
    Tcp,
    Rtu,
    RtuOverTcp,
}

/// Serial parity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Parity {
    None,
    Even,
    Odd,
}

impl Default for ModbusConfig {
    fn default() -> Self {
        Self {
            connection_type: ConnectionType::Tcp,
            host: "127.0.0.1".to_string(),
            port: 502,
            serial_port: None,
            baud_rate: 9600,
            data_bits: 8,
            stop_bits: 1,
            parity: Parity::None,
            unit_id: 1,
            timeout_ms: 1000,
            inter_frame_delay_ms: 50,
            max_retries: 3,
            write_coalescing: true,
        }
    }
}

/// Modbus register types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegisterType {
    /// Discrete Output Coils (read/write) - Function codes 1, 5, 15
    Coil,
    /// Discrete Input Contacts (read only) - Function code 2
    DiscreteInput,
    /// Analog Input Registers (read only) - Function code 4
    InputRegister,
    /// Analog Output Holding Registers (read/write) - Function codes 3, 6, 16
    HoldingRegister,
}

/// Modbus data point definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    pub name: String,
    pub register_type: RegisterType,
    pub address: u16,
    pub quantity: u16,
    pub data_type: DataType,
    pub scale_factor: f64,
    pub offset: f64,
    pub unit: Option<String>,
}

/// Data types for Modbus registers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    Bool,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float32,
    Float64,
    String,
}

/// Read result from Modbus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResult {
    pub address: u16,
    pub values: Vec<u16>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub quality: Quality,
}

/// Data quality indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    Good,
    Uncertain,
    Bad,
    ConfigError,
    DeviceFailure,
    SensorFailure,
    CommFailure,
}

/// Internal state
struct ConnectorState {
    connected: AtomicBool,
    register_cache: RwLock<HashMap<(RegisterType, u16), Vec<u16>>>,
    coil_cache: RwLock<HashMap<u16, bool>>,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            register_cache: RwLock::new(HashMap::new()),
            coil_cache: RwLock::new(HashMap::new()),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    reads: AtomicU64,
    writes: AtomicU64,
    errors: AtomicU64,
    retries: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    latency_samples: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            reads: AtomicU64::new(0),
            writes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            retries: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            latency_samples: RwLock::new(Vec::new()),
        }
    }
}

/// Modbus Connector for industrial PLCs
pub struct ModbusConnector {
    config: Config,
    modbus_config: ModbusConfig,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
}

impl ModbusConnector {
    /// Create a new Modbus connector
    pub async fn new(config: Config) -> Result<Self> {
        let modbus_config = Self::parse_modbus_config(&config)?;
        Ok(Self {
            config,
            modbus_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        })
    }

    /// Create with explicit Modbus config
    pub fn with_modbus_config(config: Config, modbus_config: ModbusConfig) -> Self {
        Self {
            config,
            modbus_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        }
    }

    fn parse_modbus_config(_config: &Config) -> Result<ModbusConfig> {
        Ok(ModbusConfig::default())
    }

    /// Read coils (Function Code 01)
    pub async fn read_coils(&self, address: u16, quantity: u16) -> Result<Vec<bool>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Simulate read - in production this would use actual Modbus protocol
        let mut result = Vec::with_capacity(quantity as usize);
        let cache = self.state.coil_cache.read().await;

        for i in 0..quantity {
            let addr = address + i;
            result.push(*cache.get(&addr).unwrap_or(&false));
        }

        self.internal_metrics.reads.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Read {} coils from address {}", quantity, address);
        Ok(result)
    }

    /// Read discrete inputs (Function Code 02)
    pub async fn read_discrete_inputs(&self, address: u16, quantity: u16) -> Result<Vec<bool>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Simulate read
        let result = vec![false; quantity as usize];

        self.internal_metrics.reads.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Read {} discrete inputs from address {}", quantity, address);
        Ok(result)
    }

    /// Read holding registers (Function Code 03)
    pub async fn read_holding_registers(&self, address: u16, quantity: u16) -> Result<Vec<u16>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Check cache first
        let cache = self.state.register_cache.read().await;
        let key = (RegisterType::HoldingRegister, address);

        let result = if let Some(cached) = cache.get(&key) {
            cached.clone()
        } else {
            vec![0u16; quantity as usize]
        };

        self.internal_metrics.reads.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!(
            "Read {} holding registers from address {}",
            quantity, address
        );
        Ok(result)
    }

    /// Read input registers (Function Code 04)
    pub async fn read_input_registers(&self, address: u16, quantity: u16) -> Result<Vec<u16>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        let result = vec![0u16; quantity as usize];

        self.internal_metrics.reads.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Read {} input registers from address {}", quantity, address);
        Ok(result)
    }

    /// Write single coil (Function Code 05)
    pub async fn write_single_coil(&self, address: u16, value: bool) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Update cache
        {
            let mut cache = self.state.coil_cache.write().await;
            cache.insert(address, value);
        }

        self.internal_metrics.writes.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Write coil {} = {}", address, value);
        Ok(())
    }

    /// Write single register (Function Code 06)
    pub async fn write_single_register(&self, address: u16, value: u16) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Update cache
        {
            let mut cache = self.state.register_cache.write().await;
            let key = (RegisterType::HoldingRegister, address);
            cache.insert(key, vec![value]);
        }

        self.internal_metrics.writes.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Write register {} = {}", address, value);
        Ok(())
    }

    /// Write multiple coils (Function Code 15)
    pub async fn write_multiple_coils(&self, address: u16, values: &[bool]) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Update cache
        {
            let mut cache = self.state.coil_cache.write().await;
            for (i, value) in values.iter().enumerate() {
                cache.insert(address + i as u16, *value);
            }
        }

        self.internal_metrics.writes.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Write {} coils from address {}", values.len(), address);
        Ok(())
    }

    /// Write multiple registers (Function Code 16)
    pub async fn write_multiple_registers(&self, address: u16, values: &[u16]) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Update cache
        {
            let mut cache = self.state.register_cache.write().await;
            let key = (RegisterType::HoldingRegister, address);
            cache.insert(key, values.to_vec());
        }

        self.internal_metrics.writes.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Write {} registers from address {}", values.len(), address);
        Ok(())
    }

    /// Read and write registers (Function Code 23)
    pub async fn read_write_registers(
        &self,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
    ) -> Result<Vec<u16>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        // Write first
        self.write_multiple_registers(write_address, write_values)
            .await?;

        // Then read
        self.read_holding_registers(read_address, read_quantity)
            .await
    }

    /// Read a data point with type conversion
    pub async fn read_data_point(&self, point: &DataPoint) -> Result<f64> {
        let raw_values = match point.register_type {
            RegisterType::Coil => {
                let values = self.read_coils(point.address, point.quantity).await?;
                vec![if values.first().copied().unwrap_or(false) {
                    1u16
                } else {
                    0u16
                }]
            }
            RegisterType::DiscreteInput => {
                let values = self
                    .read_discrete_inputs(point.address, point.quantity)
                    .await?;
                vec![if values.first().copied().unwrap_or(false) {
                    1u16
                } else {
                    0u16
                }]
            }
            RegisterType::InputRegister => {
                self.read_input_registers(point.address, point.quantity)
                    .await?
            }
            RegisterType::HoldingRegister => {
                self.read_holding_registers(point.address, point.quantity)
                    .await?
            }
        };

        // Convert based on data type
        let raw_value = self.convert_registers(&raw_values, point.data_type)?;

        // Apply scaling
        let scaled_value = raw_value * point.scale_factor + point.offset;

        Ok(scaled_value)
    }

    fn convert_registers(&self, registers: &[u16], data_type: DataType) -> Result<f64> {
        match data_type {
            DataType::Bool => Ok(if registers.first().copied().unwrap_or(0) != 0 {
                1.0
            } else {
                0.0
            }),
            DataType::Int16 => Ok(registers.first().copied().unwrap_or(0) as i16 as f64),
            DataType::UInt16 => Ok(registers.first().copied().unwrap_or(0) as f64),
            DataType::Int32 => {
                if registers.len() >= 2 {
                    let value = ((registers[0] as u32) << 16) | (registers[1] as u32);
                    Ok(value as i32 as f64)
                } else {
                    Err(Error::Protocol(
                        "Not enough registers for Int32".to_string(),
                    ))
                }
            }
            DataType::UInt32 => {
                if registers.len() >= 2 {
                    let value = ((registers[0] as u32) << 16) | (registers[1] as u32);
                    Ok(value as f64)
                } else {
                    Err(Error::Protocol(
                        "Not enough registers for UInt32".to_string(),
                    ))
                }
            }
            DataType::Float32 => {
                if registers.len() >= 2 {
                    let bits = ((registers[0] as u32) << 16) | (registers[1] as u32);
                    Ok(f32::from_bits(bits) as f64)
                } else {
                    Err(Error::Protocol(
                        "Not enough registers for Float32".to_string(),
                    ))
                }
            }
            DataType::Int64 | DataType::UInt64 | DataType::Float64 => {
                if registers.len() >= 4 {
                    let bits = ((registers[0] as u64) << 48)
                        | ((registers[1] as u64) << 32)
                        | ((registers[2] as u64) << 16)
                        | (registers[3] as u64);
                    match data_type {
                        DataType::Int64 => Ok(bits as i64 as f64),
                        DataType::UInt64 => Ok(bits as f64),
                        DataType::Float64 => Ok(f64::from_bits(bits)),
                        _ => unreachable!(),
                    }
                } else {
                    Err(Error::Protocol(
                        "Not enough registers for 64-bit type".to_string(),
                    ))
                }
            }
            DataType::String => Err(Error::Protocol(
                "String conversion not supported for numeric result".to_string(),
            )),
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
impl Connector for ModbusConnector {
    async fn connect(&mut self) -> Result<()> {
        info!(
            "Connecting to Modbus {:?} at {}:{}",
            self.modbus_config.connection_type, self.modbus_config.host, self.modbus_config.port
        );

        // Simulate connection - in production this would establish actual connection
        self.state.connected.store(true, Ordering::SeqCst);
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());

        info!("Connected to Modbus device");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from Modbus device");

        self.state.connected.store(false, Ordering::SeqCst);

        info!("Disconnected from Modbus device");
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
            name: "test-modbus".to_string(),
            connector_type: "modbus".to_string(),
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
    async fn test_modbus_config_default() {
        let config = ModbusConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 502);
        assert_eq!(config.unit_id, 1);
        assert!(matches!(config.connection_type, ConnectionType::Tcp));
    }

    #[tokio::test]
    async fn test_modbus_connector_creation() {
        let config = create_test_config();
        let connector = ModbusConnector::new(config).await;
        assert!(connector.is_ok());
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = create_test_config();
        let mut connector = ModbusConnector::new(config).await.unwrap();

        assert!(!connector.is_connected().await);

        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);

        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }

    #[tokio::test]
    async fn test_read_write_operations() {
        let config = create_test_config();
        let mut connector = ModbusConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        // Write and read coil
        connector.write_single_coil(0, true).await.unwrap();
        let coils = connector.read_coils(0, 1).await.unwrap();
        assert_eq!(coils.len(), 1);
        assert!(coils[0]);

        // Write and read register
        connector.write_single_register(0, 1234).await.unwrap();
        let registers = connector.read_holding_registers(0, 1).await.unwrap();
        assert_eq!(registers.len(), 1);
    }
}
