//! GaussTwin Co-Simulation Framework
//!
//! Provides a unified co-simulation framework supporting both FMI 2.0 and HLA IEEE-1516e standards.
//! This crate enables seamless integration of different simulation models and federates through
//! standardized interfaces and protocols.
//!
//! # Features
//!
//! - FMI 2.0 Support:
//!   - Model Exchange
//!   - Co-Simulation
//!   - Import/Export capabilities
//!   - Variable access and manipulation
//!
//! - HLA IEEE-1516e Support:
//!   - Federation management
//!   - Object/Attribute management
//!   - Time management
//!   - Data Distribution Management
//!
//! - Common Infrastructure:
//!   - Time synchronization
//!   - Data exchange
//!   - State management
//!   - Event handling
//!   - Error recovery

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use metrics::{counter, gauge};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

// Re-exports
pub use crate::common::data::DataValue;
pub use crate::common::sync::SyncMode;
pub use crate::common::time::SimulationTime;

// Modules
pub mod common;
pub mod fmi;
pub mod hla;

// Error types
#[derive(Error, Debug)]
pub enum CosimError {
    #[error("Time synchronization error: {0}")]
    TimeSync(String),

    #[error("Data exchange error: {0}")]
    DataExchange(String),

    #[error("FMI error: {0}")]
    Fmi(#[from] fmi::FmiError),

    #[error("HLA error: {0}")]
    Hla(#[from] hla::HlaError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Runtime error: {0}")]
    Runtime(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, CosimError>;

/// Core co-simulation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosimConfig {
    /// Simulation mode (FMI or HLA)
    pub mode: CosimMode,

    /// Time management configuration
    pub time_config: TimeConfig,

    /// Data exchange configuration
    pub data_config: DataConfig,

    /// Logging configuration
    pub logging_config: LoggingConfig,
}

/// Co-simulation modes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CosimMode {
    /// FMI 2.0 mode
    Fmi(fmi::FmiConfig),

    /// HLA IEEE-1516e mode
    Hla(hla::HlaConfig),
}

/// Time management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeConfig {
    /// Simulation start time
    pub start_time: DateTime<Utc>,

    /// Simulation end time (if any)
    pub end_time: Option<DateTime<Utc>>,

    /// Time step size
    pub step_size: Duration,

    /// Synchronization mode
    pub sync_mode: SyncMode,

    /// Lookahead time (for HLA)
    pub lookahead: Option<Duration>,
}

/// Data exchange configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataConfig {
    /// Maximum message size
    pub max_message_size: usize,

    /// Buffer capacity
    pub buffer_capacity: usize,

    /// Data validation rules
    pub validation_rules: Vec<ValidationRule>,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,

    /// Log file path
    pub file_path: Option<String>,

    /// Enable metrics collection
    pub enable_metrics: bool,
}

/// Data validation rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    /// Rule name
    pub name: String,

    /// Variable pattern to match
    pub pattern: String,

    /// Validation expression
    pub expression: String,
}

/// Core co-simulation manager trait
#[async_trait]
pub trait CosimManager: Send + Sync {
    /// Initialize the co-simulation
    async fn initialize(&mut self, config: CosimConfig) -> Result<()>;

    /// Step the simulation forward
    async fn step(&mut self) -> Result<()>;

    /// Get current simulation time
    fn current_time(&self) -> SimulationTime;

    /// Exchange data between simulators
    async fn exchange_data(&mut self, data: HashMap<String, DataValue>) -> Result<()>;

    /// Handle simulation events
    async fn handle_event(&mut self, event: SimulationEvent) -> Result<()>;

    /// Cleanup and shutdown
    async fn shutdown(&mut self) -> Result<()>;
}

/// Simulation event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimulationEvent {
    /// Time event
    Time(SimulationTime),

    /// State change event
    StateChange {
        entity_id: String,
        old_state: String,
        new_state: String,
    },

    /// Data update event
    DataUpdate {
        variable: String,
        value: DataValue,
        timestamp: SimulationTime,
    },

    /// Error event
    Error {
        source: String,
        message: String,
        severity: ErrorSeverity,
    },
}

/// Error severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

// Implementations will be in respective module files
