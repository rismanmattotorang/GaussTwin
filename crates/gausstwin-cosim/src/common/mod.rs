//! Common functionality shared between FMI and HLA implementations
//!
//! This module provides core data structures and algorithms used by both
//! FMI and HLA implementations.

pub mod data;
pub mod event;
pub mod federation;
pub mod model;
pub mod sync;
pub mod time;

use std::{collections::HashMap, sync::Arc, time::Duration};

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::common::data::DataValue;
use crate::common::sync::SyncManager;
use crate::common::sync::SyncMode;
use crate::common::time::SimulationTime;
use crate::common::time::TimeManager;
use crate::Result;
use std::collections::HashSet;

/// Anti-message handling policies
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AntiMessagePolicy {
    /// Aggressive cancellation - send anti-messages immediately
    Aggressive,
    /// Lazy cancellation - wait to see if re-execution produces same message
    Lazy,
    /// Adaptive - switch between aggressive and lazy based on rollback frequency
    Adaptive,
}

/// Interpolation methods for time-stepped sync
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InterpolationMethod {
    /// Linear interpolation
    Linear,
    /// Cubic spline interpolation
    CubicSpline,
    /// Nearest neighbor
    NearestNeighbor,
}

/// Shared state between simulators with advanced features
pub struct SharedState {
    /// Variable values with versioning
    values: DashMap<String, VersionedValue>,

    /// Event channels for different priorities
    events: EventChannels,

    /// Time manager
    time_manager: Arc<RwLock<TimeManager>>,

    /// Statistics and monitoring
    stats: Arc<SimulationStats>,

    /// Data exchange optimization
    data_router: Arc<DataRouter>,

    /// Synchronization manager
    sync_manager: Arc<RwLock<SyncManager>>,
}

/// Versioned variable value for rollback support
#[derive(Debug, Clone)]
pub struct VersionedValue {
    /// Current value
    pub value: DataValue,
    /// Version history for rollback
    pub history: Vec<(SimulationTime, DataValue)>,
    /// Metadata
    pub metadata: VariableMetadata,
}

/// Event channels with priority levels
#[derive(Debug)]
pub struct EventChannels {
    /// High priority events (e.g. time management)
    high: broadcast::Sender<SimulationEvent>,
    /// Normal priority events
    normal: broadcast::Sender<SimulationEvent>,
    /// Low priority events (e.g. monitoring)
    low: broadcast::Sender<SimulationEvent>,
}

/// Data routing and optimization
#[derive(Debug)]
pub struct DataRouter {
    /// Routing tables for efficient data distribution
    routes: DashMap<String, Vec<String>>,
    /// Data dependencies
    dependencies: DashMap<String, HashSet<String>>,
    /// Caching configuration
    cache_config: CacheConfig,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum cache size
    pub max_size: usize,
    /// Cache eviction policy
    pub eviction_policy: CacheEvictionPolicy,
    /// Prefetch settings
    pub prefetch: bool,
}

/// Cache eviction policies
#[derive(Debug, Clone, Copy)]
pub enum CacheEvictionPolicy {
    LRU,
    LFU,
    FIFO,
}

impl Default for CacheConfig {
    fn default() -> Self {
        CacheConfig {
            max_size: 1024,
            eviction_policy: CacheEvictionPolicy::LRU,
            prefetch: false,
        }
    }
}

impl SharedState {
    /// Create new shared state with advanced features
    pub fn new() -> Self {
        let (high_tx, _) = broadcast::channel(1000);
        let (normal_tx, _) = broadcast::channel(1000);
        let (low_tx, _) = broadcast::channel(1000);

        let sync_mode = crate::common::sync::SyncMode::Conservative {
            lookahead: Duration::from_secs(1),
            min_step: Duration::from_secs(1),
            max_lag: Duration::from_secs(1),
        };
        let sync_manager = Arc::new(RwLock::new(SyncManager::new(sync_mode, 1)));

        Self {
            values: DashMap::new(),
            events: EventChannels {
                high: high_tx,
                normal: normal_tx,
                low: low_tx,
            },
            time_manager: Arc::new(RwLock::new(TimeManager::new())),
            stats: Arc::new(SimulationStats::default()),
            data_router: Arc::new(DataRouter::new()),
            sync_manager,
        }
    }

    /// Get variable value with version control
    pub fn get_value(&self, name: &str, time: SimulationTime) -> Option<DataValue> {
        self.values
            .get(name)
            .and_then(|v| v.get_value_at(time).cloned())
    }

    /// Set variable value with versioning
    pub fn set_value(&self, name: String, value: DataValue, time: SimulationTime) {
        if let Some(mut v) = self.values.get_mut(&name) {
            v.add_version(time, value);
        } else {
            let mut versioned = VersionedValue::new(value.clone());
            versioned.add_version(time, value);
            self.values.insert(name.clone(), versioned);
        }

        // Notify dependents through data router
        self.data_router.notify_dependents(&name);
    }

    /// Subscribe to events with priority
    pub fn subscribe(&self, priority: EventPriority) -> broadcast::Receiver<SimulationEvent> {
        match priority {
            EventPriority::High => self.events.high.subscribe(),
            EventPriority::Normal => self.events.normal.subscribe(),
            EventPriority::Low => self.events.low.subscribe(),
        }
    }

    /// Publish event with priority
    pub fn publish(&self, event: SimulationEvent, priority: EventPriority) -> Result<()> {
        let sender = match priority {
            EventPriority::High => &self.events.high,
            EventPriority::Normal => &self.events.normal,
            EventPriority::Low => &self.events.low,
        };

        sender
            .send(event)
            .map_err(|e| CosimError::Runtime(format!("Failed to publish event: {}", e)))?;
        Ok(())
    }
}

/// Event priority levels
#[derive(Debug, Clone, Copy)]
pub enum EventPriority {
    High,
    Normal,
    Low,
}

/// Simulation statistics
#[derive(Debug, Default)]
pub struct SimulationStats {
    /// Number of steps executed
    pub steps: AtomicUsize,

    /// Number of events processed
    pub events: AtomicUsize,

    /// Number of data exchanges
    pub exchanges: AtomicUsize,

    /// Total simulation time
    pub total_time: AtomicU64,

    /// Number of errors
    pub errors: AtomicUsize,
}

/// Variable metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableMetadata {
    /// Variable name
    pub name: String,

    /// Variable type
    pub var_type: VariableType,

    /// Unit (if applicable)
    pub unit: Option<String>,

    /// Description
    pub description: Option<String>,

    /// Causality
    pub causality: Causality,

    /// Variability
    pub variability: Variability,
}

/// Variable types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VariableType {
    Real,
    Integer,
    Boolean,
    String,
    Enumeration(Vec<String>),
    Binary,
}

/// Variable causality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Causality {
    Parameter,
    CalculatedParameter,
    Input,
    Output,
    Local,
    Independent,
}

/// Variable variability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Variability {
    Constant,
    Fixed,
    Tunable,
    Discrete,
    Continuous,
}

/// Time management status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeStatus {
    /// Time is granted
    Granted,

    /// Time advance is pending
    Pending,

    /// Time advance was rejected
    Rejected,
}

/// Simulation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationEvent {
    /// Event ID
    pub id: Uuid,

    /// Event timestamp
    pub timestamp: SimulationTime,

    /// Event type
    pub event_type: EventType,

    /// Event data
    pub data: HashMap<String, DataValue>,
}

/// Event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    /// Time event
    Time,

    /// State event
    State,

    /// Step event
    Step,

    /// External event
    External,
}

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::CosimError;

impl DataRouter {
    pub fn new() -> Self {
        DataRouter {
            routes: DashMap::new(),
            dependencies: DashMap::new(),
            cache_config: CacheConfig::default(),
        }
    }
    pub fn notify_dependents(&self, _name: &str) {
        // stub
    }
}

impl VersionedValue {
    pub fn new(value: DataValue) -> Self {
        VersionedValue {
            value,
            history: Vec::new(),
            metadata: VariableMetadata {
                name: String::new(),
                var_type: VariableType::Real,
                unit: None,
                description: None,
                causality: Causality::Parameter,
                variability: Variability::Constant,
            },
        }
    }
    pub fn get_value_at(&self, _time: SimulationTime) -> Option<&DataValue> {
        self.history.last().map(|(_, v)| v)
    }
    pub fn add_version(&mut self, _time: SimulationTime, value: DataValue) {
        self.history.push((_time, value));
    }
}
