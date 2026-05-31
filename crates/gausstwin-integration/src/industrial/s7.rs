//! Siemens S7 Connector
//!
//! Provides connectivity to Siemens S7 PLCs (S7-300, S7-400, S7-1200, S7-1500)
//! using the S7 protocol for reading and writing data blocks, inputs, outputs, and markers.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// S7-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S7Config {
    /// PLC IP address
    pub host: String,
    /// PLC port (default 102)
    pub port: u16,
    /// Rack number
    pub rack: u8,
    /// Slot number
    pub slot: u8,
    /// Connection type
    pub connection_type: ConnectionType,
    /// PDU size
    pub pdu_size: u16,
    /// Connection timeout in milliseconds
    pub timeout_ms: u64,
    /// Keep alive interval in milliseconds
    pub keep_alive_ms: u64,
    /// Auto reconnect
    pub auto_reconnect: bool,
    /// Maximum reconnect attempts
    pub max_reconnect_attempts: u32,
}

/// S7 connection type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionType {
    /// S7 Basic (S7-200, S7-1200, S7-1500)
    Basic,
    /// S7-300/400 OP (Operator Panel)
    OP,
    /// S7-300/400 S7Basic
    S7Basic,
}

impl Default for S7Config {
    fn default() -> Self {
        Self {
            host: "192.168.0.1".to_string(),
            port: 102,
            rack: 0,
            slot: 1,
            connection_type: ConnectionType::Basic,
            pdu_size: 480,
            timeout_ms: 5000,
            keep_alive_ms: 10000,
            auto_reconnect: true,
            max_reconnect_attempts: 3,
        }
    }
}

/// S7 area types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Area {
    /// Process inputs (I)
    PE,
    /// Process outputs (Q)
    PA,
    /// Markers (M)
    MK,
    /// Data blocks (DB)
    DB,
    /// Counters (C)
    CT,
    /// Timers (T)
    TM,
}

impl Area {
    pub fn code(&self) -> u8 {
        match self {
            Area::PE => 0x81,
            Area::PA => 0x82,
            Area::MK => 0x83,
            Area::DB => 0x84,
            Area::CT => 0x1C,
            Area::TM => 0x1D,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Area::PE => "PE (Inputs)",
            Area::PA => "PA (Outputs)",
            Area::MK => "MK (Markers)",
            Area::DB => "DB (Data Block)",
            Area::CT => "CT (Counters)",
            Area::TM => "TM (Timers)",
        }
    }
}

/// Word length for S7 data types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WordLen {
    Bit,
    Byte,
    Char,
    Word,
    Int,
    DWord,
    DInt,
    Real,
    Date,
    Tod,
    Time,
    S5Time,
    DateTime,
    Counter,
    Timer,
}

impl WordLen {
    pub fn size(&self) -> usize {
        match self {
            WordLen::Bit => 1,
            WordLen::Byte | WordLen::Char => 1,
            WordLen::Word | WordLen::Int | WordLen::Counter | WordLen::Timer | WordLen::S5Time => 2,
            WordLen::DWord
            | WordLen::DInt
            | WordLen::Real
            | WordLen::Time
            | WordLen::Tod
            | WordLen::Date => 4,
            WordLen::DateTime => 8,
        }
    }
}

/// S7 data tag definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S7Tag {
    pub name: String,
    pub area: Area,
    pub db_number: u16,
    pub start: u32,
    pub bit: u8,
    pub word_len: WordLen,
    pub count: u16,
}

impl S7Tag {
    /// Create a DB tag
    pub fn db(name: &str, db: u16, start: u32, word_len: WordLen) -> Self {
        Self {
            name: name.to_string(),
            area: Area::DB,
            db_number: db,
            start,
            bit: 0,
            word_len,
            count: 1,
        }
    }

    /// Create a marker tag
    pub fn marker(name: &str, start: u32, word_len: WordLen) -> Self {
        Self {
            name: name.to_string(),
            area: Area::MK,
            db_number: 0,
            start,
            bit: 0,
            word_len,
            count: 1,
        }
    }

    /// Create an input tag
    pub fn input(name: &str, start: u32, word_len: WordLen) -> Self {
        Self {
            name: name.to_string(),
            area: Area::PE,
            db_number: 0,
            start,
            bit: 0,
            word_len,
            count: 1,
        }
    }

    /// Create an output tag
    pub fn output(name: &str, start: u32, word_len: WordLen) -> Self {
        Self {
            name: name.to_string(),
            area: Area::PA,
            db_number: 0,
            start,
            bit: 0,
            word_len,
            count: 1,
        }
    }
}

/// S7 data value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum S7Value {
    Bool(bool),
    Byte(u8),
    Char(char),
    Word(u16),
    Int(i16),
    DWord(u32),
    DInt(i32),
    Real(f32),
    LReal(f64),
    Bytes(Vec<u8>),
    String(String),
}

impl S7Value {
    pub fn as_bytes(&self) -> Vec<u8> {
        match self {
            S7Value::Bool(v) => vec![if *v { 1 } else { 0 }],
            S7Value::Byte(v) => vec![*v],
            S7Value::Char(v) => vec![*v as u8],
            S7Value::Word(v) => v.to_be_bytes().to_vec(),
            S7Value::Int(v) => v.to_be_bytes().to_vec(),
            S7Value::DWord(v) => v.to_be_bytes().to_vec(),
            S7Value::DInt(v) => v.to_be_bytes().to_vec(),
            S7Value::Real(v) => v.to_be_bytes().to_vec(),
            S7Value::LReal(v) => v.to_be_bytes().to_vec(),
            S7Value::Bytes(v) => v.clone(),
            S7Value::String(v) => v.as_bytes().to_vec(),
        }
    }
}

/// Read result
#[derive(Debug, Clone)]
pub struct ReadResult {
    pub tag: S7Tag,
    pub value: S7Value,
    pub quality: Quality,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Data quality
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    Good,
    Bad,
    Uncertain,
}

/// CPU info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub module_type: String,
    pub serial_number: String,
    pub as_name: String,
    pub copyright: String,
    pub module_name: String,
}

/// CPU state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuState {
    Unknown,
    Stop,
    Run,
}

/// Internal state
struct ConnectorState {
    connected: AtomicBool,
    cpu_state: RwLock<CpuState>,
    data_blocks: RwLock<HashMap<u16, Vec<u8>>>,
    markers: RwLock<Vec<u8>>,
    inputs: RwLock<Vec<u8>>,
    outputs: RwLock<Vec<u8>>,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            cpu_state: RwLock::new(CpuState::Unknown),
            data_blocks: RwLock::new(HashMap::new()),
            markers: RwLock::new(vec![0u8; 1024]),
            inputs: RwLock::new(vec![0u8; 1024]),
            outputs: RwLock::new(vec![0u8; 1024]),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    reads: AtomicU64,
    writes: AtomicU64,
    bytes_read: AtomicU64,
    bytes_written: AtomicU64,
    errors: AtomicU64,
    reconnections: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    latency_samples: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            reads: AtomicU64::new(0),
            writes: AtomicU64::new(0),
            bytes_read: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            reconnections: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            latency_samples: RwLock::new(Vec::new()),
        }
    }
}

/// S7 Connector
pub struct S7Connector {
    config: Config,
    s7_config: S7Config,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
}

impl S7Connector {
    /// Create a new S7 connector
    pub async fn new(config: Config) -> Result<Self> {
        let s7_config = S7Config::default();
        Ok(Self {
            config,
            s7_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        })
    }

    /// Create with explicit S7 config
    pub fn with_s7_config(config: Config, s7_config: S7Config) -> Self {
        Self {
            config,
            s7_config,
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

    /// Read a single tag
    pub async fn read_tag(&self, tag: &S7Tag) -> Result<ReadResult> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        let value = self
            .read_area(tag.area, tag.db_number, tag.start, tag.word_len, tag.count)
            .await?;

        self.internal_metrics.reads.fetch_add(1, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(ReadResult {
            tag: tag.clone(),
            value,
            quality: Quality::Good,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Read multiple tags
    pub async fn read_tags(&self, tags: &[S7Tag]) -> Result<Vec<ReadResult>> {
        let mut results = Vec::with_capacity(tags.len());
        for tag in tags {
            results.push(self.read_tag(tag).await?);
        }
        Ok(results)
    }

    /// Read from an area
    async fn read_area(
        &self,
        area: Area,
        db_number: u16,
        start: u32,
        word_len: WordLen,
        count: u16,
    ) -> Result<S7Value> {
        let bytes = match area {
            Area::DB => {
                let dbs = self.state.data_blocks.read().await;
                let db = dbs
                    .get(&db_number)
                    .ok_or_else(|| Error::NotFound(format!("DB{} not found", db_number)))?;
                let end = start as usize + word_len.size() * count as usize;
                if end > db.len() {
                    return Err(Error::Protocol("Read beyond DB size".to_string()));
                }
                db[start as usize..end].to_vec()
            }
            Area::MK => {
                let markers = self.state.markers.read().await;
                let end = start as usize + word_len.size() * count as usize;
                markers[start as usize..end].to_vec()
            }
            Area::PE => {
                let inputs = self.state.inputs.read().await;
                let end = start as usize + word_len.size() * count as usize;
                inputs[start as usize..end].to_vec()
            }
            Area::PA => {
                let outputs = self.state.outputs.read().await;
                let end = start as usize + word_len.size() * count as usize;
                outputs[start as usize..end].to_vec()
            }
            _ => vec![0u8; word_len.size() * count as usize],
        };

        self.internal_metrics
            .bytes_read
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);

        // Convert bytes to value
        Ok(self.bytes_to_value(&bytes, word_len))
    }

    fn bytes_to_value(&self, bytes: &[u8], word_len: WordLen) -> S7Value {
        match word_len {
            WordLen::Bit => S7Value::Bool(bytes.first().copied().unwrap_or(0) != 0),
            WordLen::Byte => S7Value::Byte(bytes.first().copied().unwrap_or(0)),
            WordLen::Char => S7Value::Char(bytes.first().copied().unwrap_or(0) as char),
            WordLen::Word => {
                let arr: [u8; 2] = bytes[..2].try_into().unwrap_or([0; 2]);
                S7Value::Word(u16::from_be_bytes(arr))
            }
            WordLen::Int => {
                let arr: [u8; 2] = bytes[..2].try_into().unwrap_or([0; 2]);
                S7Value::Int(i16::from_be_bytes(arr))
            }
            WordLen::DWord => {
                let arr: [u8; 4] = bytes[..4].try_into().unwrap_or([0; 4]);
                S7Value::DWord(u32::from_be_bytes(arr))
            }
            WordLen::DInt => {
                let arr: [u8; 4] = bytes[..4].try_into().unwrap_or([0; 4]);
                S7Value::DInt(i32::from_be_bytes(arr))
            }
            WordLen::Real => {
                let arr: [u8; 4] = bytes[..4].try_into().unwrap_or([0; 4]);
                S7Value::Real(f32::from_be_bytes(arr))
            }
            _ => S7Value::Bytes(bytes.to_vec()),
        }
    }

    /// Write a single tag
    pub async fn write_tag(&self, tag: &S7Tag, value: S7Value) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();
        let bytes = value.as_bytes();

        self.write_area(tag.area, tag.db_number, tag.start, &bytes)
            .await?;

        self.internal_metrics.writes.fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .bytes_written
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Write tag {} = {:?}", tag.name, value);
        Ok(())
    }

    /// Write to an area
    async fn write_area(&self, area: Area, db_number: u16, start: u32, data: &[u8]) -> Result<()> {
        match area {
            Area::DB => {
                let mut dbs = self.state.data_blocks.write().await;
                let db = dbs.entry(db_number).or_insert_with(|| vec![0u8; 65536]);
                let end = start as usize + data.len();
                if end > db.len() {
                    db.resize(end, 0);
                }
                db[start as usize..end].copy_from_slice(data);
            }
            Area::MK => {
                let mut markers = self.state.markers.write().await;
                let end = start as usize + data.len();
                markers[start as usize..end].copy_from_slice(data);
            }
            Area::PA => {
                let mut outputs = self.state.outputs.write().await;
                let end = start as usize + data.len();
                outputs[start as usize..end].copy_from_slice(data);
            }
            _ => {
                return Err(Error::Protocol(format!(
                    "Cannot write to area {}",
                    area.name()
                )));
            }
        }
        Ok(())
    }

    /// Get CPU info
    pub async fn get_cpu_info(&self) -> Result<CpuInfo> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        Ok(CpuInfo {
            module_type: format!(
                "S7-{}",
                if self.s7_config.slot == 1 {
                    "1500"
                } else {
                    "300"
                }
            ),
            serial_number: "S7-XXXX-XXXX-XXXX".to_string(),
            as_name: "GaussTwin Simulated PLC".to_string(),
            copyright: "Copyright Siemens AG".to_string(),
            module_name: "CPU 1515-2 PN".to_string(),
        })
    }

    /// Get CPU state
    pub async fn get_cpu_state(&self) -> Result<CpuState> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let state = self.state.cpu_state.read().await;
        Ok(*state)
    }

    /// Start the CPU
    pub async fn start_cpu(&self) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let mut state = self.state.cpu_state.write().await;
        *state = CpuState::Run;
        info!("CPU started");
        Ok(())
    }

    /// Stop the CPU
    pub async fn stop_cpu(&self) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let mut state = self.state.cpu_state.write().await;
        *state = CpuState::Stop;
        info!("CPU stopped");
        Ok(())
    }

    /// Create a data block
    pub async fn create_db(&self, db_number: u16, size: usize) -> Result<()> {
        let mut dbs = self.state.data_blocks.write().await;
        dbs.insert(db_number, vec![0u8; size]);
        info!("Created DB{} with size {}", db_number, size);
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
            connection_failures: self.internal_metrics.reconnections.load(Ordering::Relaxed),
            messages_sent: self.internal_metrics.writes.load(Ordering::Relaxed),
            messages_received: self.internal_metrics.reads.load(Ordering::Relaxed),
            errors: self.internal_metrics.errors.load(Ordering::Relaxed),
            average_latency_ms: avg_latency,
            bytes_sent: self.internal_metrics.bytes_written.load(Ordering::Relaxed),
            bytes_received: self.internal_metrics.bytes_read.load(Ordering::Relaxed),
            uptime_seconds: uptime,
        }
    }
}

#[async_trait]
impl Connector for S7Connector {
    async fn connect(&mut self) -> Result<()> {
        info!(
            "Connecting to S7 PLC at {}:{} (Rack {}, Slot {})",
            self.s7_config.host, self.s7_config.port, self.s7_config.rack, self.s7_config.slot
        );

        self.state.connected.store(true, Ordering::SeqCst);
        *self.state.cpu_state.write().await = CpuState::Run;
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());

        // Create default DBs
        self.create_db(1, 1024).await?;

        info!("Connected to S7 PLC");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from S7 PLC");
        self.state.connected.store(false, Ordering::SeqCst);
        *self.state.cpu_state.write().await = CpuState::Unknown;
        info!("Disconnected from S7 PLC");
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
            name: "test-s7".to_string(),
            connector_type: "s7".to_string(),
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
    async fn test_s7_config_default() {
        let config = S7Config::default();
        assert_eq!(config.port, 102);
        assert_eq!(config.rack, 0);
        assert_eq!(config.slot, 1);
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = create_test_config();
        let mut connector = S7Connector::new(config).await.unwrap();

        assert!(!connector.is_connected().await);
        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);
        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }

    #[tokio::test]
    async fn test_read_write_tag() {
        let config = create_test_config();
        let mut connector = S7Connector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let tag = S7Tag::db("test_int", 1, 0, WordLen::Int);

        // Write value
        connector
            .write_tag(&tag, S7Value::Int(12345))
            .await
            .unwrap();

        // Read back
        let result = connector.read_tag(&tag).await.unwrap();
        match result.value {
            S7Value::Int(v) => assert_eq!(v, 12345),
            _ => panic!("Unexpected value type"),
        }
    }

    #[tokio::test]
    async fn test_cpu_operations() {
        let config = create_test_config();
        let mut connector = S7Connector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let info = connector.get_cpu_info().await.unwrap();
        assert!(!info.module_type.is_empty());

        let state = connector.get_cpu_state().await.unwrap();
        assert_eq!(state, CpuState::Run);

        connector.stop_cpu().await.unwrap();
        assert_eq!(connector.get_cpu_state().await.unwrap(), CpuState::Stop);

        connector.start_cpu().await.unwrap();
        assert_eq!(connector.get_cpu_state().await.unwrap(), CpuState::Run);
    }
}
