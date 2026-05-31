//! GaussTwin Discrete Event Simulation
//!
//! A high-performance discrete event simulation engine with features including:
//! - Priority-based event scheduling
//! - Parallel event execution
//! - Event dependencies and causality tracking
//! - Rollback support
//! - Comprehensive metrics and monitoring
//!
//! # Features
//! - Multiple event priorities
//! - Parallel event processing
//! - Event dependency management
//! - State checkpointing
//! - Performance metrics
//! - OpenTelemetry integration
//!
//! # Examples
//! ```no_run
//! use gausstwin_des::{DiscreteEventSimulator, SimulationConfig, Event, Priority};
//! use gausstwin_des::SimulationError;
//!
//! async fn example() -> Result<(), SimulationError> {
//!     let config = SimulationConfig {
//!         max_time: 100.0,
//!         max_events: Some(1000),
//!         parallel_execution: true,
//!         max_concurrent_events: 4,
//!         checkpoint_interval: None,
//!         metrics_enabled: true,
//!     };
//!     
//!     let simulator = DiscreteEventSimulator::new(config);
//!     simulator.run().await?;
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crossbeam::queue::SegQueue;
use dashmap::DashMap;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use priority_queue::PriorityQueue;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration as StdDuration,
};
use thiserror::Error;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Comprehensive error type for simulation operations
#[derive(Debug, Error)]
pub enum SimulationError {
    /// Errors related to time management
    #[error("Time error: {kind:?}")]
    Time {
        kind: TimeErrorKind,
        message: String,
    },

    /// Errors related to event handling
    #[error("Event error: {kind:?}")]
    Event {
        kind: EventErrorKind,
        message: String,
        event_id: Option<Uuid>,
    },

    /// Errors related to state management
    #[error("State error: {kind:?}")]
    State {
        kind: StateErrorKind,
        message: String,
    },

    /// Errors related to resource management
    #[error("Resource error: {kind:?}")]
    Resource {
        kind: ResourceErrorKind,
        message: String,
    },

    /// Errors related to parallel execution
    #[error("Execution error: {kind:?}")]
    Execution {
        kind: ExecutionErrorKind,
        message: String,
    },

    /// Internal errors
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Types of time-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeErrorKind {
    /// Time value is invalid (e.g., negative)
    InvalidValue,
    /// Time is out of bounds
    OutOfBounds,
    /// Time causality violation
    CausalityViolation,
}

/// Types of event-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventErrorKind {
    /// Event not found
    NotFound,
    /// Invalid event data
    InvalidData,
    /// Event dependency error
    DependencyError,
    /// Event timeout
    Timeout,
    /// Event retry limit exceeded
    RetryLimitExceeded,
}

/// Types of state-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateErrorKind {
    /// Invalid state transition
    InvalidTransition,
    /// State not found
    NotFound,
    /// Checkpoint error
    CheckpointError,
    /// Rollback error
    RollbackError,
}

/// Types of resource-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceErrorKind {
    /// Resource not available
    NotAvailable,
    /// Resource limit exceeded
    LimitExceeded,
    /// Resource allocation error
    AllocationError,
}

/// Types of execution-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionErrorKind {
    /// Parallel execution error
    ParallelError,
    /// Task scheduling error
    SchedulingError,
    /// Task cancellation error
    CancellationError,
}

impl SimulationError {
    /// Create a new time error
    pub fn time_error(kind: TimeErrorKind, message: impl Into<String>) -> Self {
        SimulationError::Time {
            kind,
            message: message.into(),
        }
    }

    /// Create a new event error
    pub fn event_error(
        kind: EventErrorKind,
        message: impl Into<String>,
        event_id: Option<Uuid>,
    ) -> Self {
        SimulationError::Event {
            kind,
            message: message.into(),
            event_id,
        }
    }

    /// Create a new state error
    pub fn state_error(kind: StateErrorKind, message: impl Into<String>) -> Self {
        SimulationError::State {
            kind,
            message: message.into(),
        }
    }

    /// Create a new resource error
    pub fn resource_error(kind: ResourceErrorKind, message: impl Into<String>) -> Self {
        SimulationError::Resource {
            kind,
            message: message.into(),
        }
    }

    /// Create a new execution error
    pub fn execution_error(kind: ExecutionErrorKind, message: impl Into<String>) -> Self {
        SimulationError::Execution {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Priority {
    Critical = 0,
    High = 1,
    Normal = 5,
    Low = 10,
    Background = 15,
}

impl Priority {
    pub fn from_value(value: i32) -> Option<Self> {
        match value {
            0 => Some(Priority::Critical),
            1 => Some(Priority::High),
            5 => Some(Priority::Normal),
            10 => Some(Priority::Low),
            15 => Some(Priority::Background),
            _ => None,
        }
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> Ordering {
        (*self as i32).cmp(&(*other as i32))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub time: f64,
    pub priority: Priority,
    pub data: EventData,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub dependencies: Vec<Uuid>,
    pub causality_chain: Vec<Uuid>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub timeout: Option<StdDuration>,
    pub rollback_handler: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventData {
    AgentAction {
        agent_id: Uuid,
        action: String,
        parameters: Option<serde_json::Value>,
    },
    StateChange {
        entity_id: Uuid,
        state: String,
        value: serde_json::Value,
    },
    Interaction {
        source_id: Uuid,
        target_id: Uuid,
        interaction_type: String,
        data: serde_json::Value,
    },
    SystemEvent {
        event_type: String,
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventStats {
    pub total_events: u64,
    pub processed_events: u64,
    pub pending_events: u64,
    pub avg_processing_time_ms: f64,
    pub events_per_second: f64,
    pub priority_distribution: HashMap<Priority, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub max_time: f64,
    pub max_events: Option<u64>,
    pub parallel_execution: bool,
    pub max_concurrent_events: usize,
    pub checkpoint_interval: Option<StdDuration>,
    pub metrics_enabled: bool,
}

#[derive(Debug)]
pub struct EventQueue {
    queue: Arc<RwLock<PriorityQueue<Uuid, (OrderedFloat<f64>, Priority)>>>,
    events: Arc<DashMap<Uuid, Event>>,
    stats: Arc<RwLock<EventStats>>,
    #[allow(dead_code)]
    parallel_queue: Arc<SegQueue<Event>>,
    semaphore: Arc<Semaphore>,
}

impl EventQueue {
    pub fn new(max_concurrent_events: usize) -> Self {
        Self {
            queue: Arc::new(RwLock::new(PriorityQueue::new())),
            events: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(EventStats {
                total_events: 0,
                processed_events: 0,
                pending_events: 0,
                avg_processing_time_ms: 0.0,
                events_per_second: 0.0,
                priority_distribution: HashMap::new(),
            })),
            parallel_queue: Arc::new(SegQueue::new()),
            semaphore: Arc::new(Semaphore::new(max_concurrent_events)),
        }
    }

    pub fn schedule_event(&self, event: Event) -> Result<(), SimulationError> {
        if event.time < 0.0 {
            return Err(SimulationError::time_error(
                TimeErrorKind::InvalidValue,
                "Event time cannot be negative",
            ));
        }

        let mut queue = self.queue.write();
        let mut stats = self.stats.write();

        let event_priority = event.priority;
        queue.push(event.id, (OrderedFloat(event.time), event_priority));
        self.events.insert(event.id, event);

        stats.total_events += 1;
        stats.pending_events += 1;
        *stats
            .priority_distribution
            .entry(event_priority)
            .or_insert(0) += 1;

        Ok(())
    }

    pub fn peek_next_event(&self) -> Option<Event> {
        let queue = self.queue.read();
        queue
            .peek()
            .map(|(id, _)| self.events.get(id).unwrap().clone())
    }

    pub fn pop_next_event(&self) -> Option<Event> {
        let mut queue = self.queue.write();
        let mut stats = self.stats.write();

        if let Some((id, _)) = queue.pop() {
            if let Some((_, event)) = self.events.remove(&id) {
                stats.pending_events -= 1;
                stats.processed_events += 1;
                Some(event)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn cancel_event(&self, event_id: Uuid) -> Result<(), SimulationError> {
        let mut queue = self.queue.write();
        let mut stats = self.stats.write();

        if let Some((_, event)) = self.events.remove(&event_id) {
            queue.remove(&event_id);
            stats.pending_events -= 1;
            *stats
                .priority_distribution
                .entry(event.priority)
                .or_insert(0) -= 1;
            Ok(())
        } else {
            Err(SimulationError::event_error(
                EventErrorKind::NotFound,
                format!("Event {} not found", event_id),
                Some(event_id),
            ))
        }
    }

    pub async fn process_parallel_events<F>(&self, processor: F) -> Result<(), SimulationError>
    where
        F: Fn(Event) -> Result<(), SimulationError> + Send + Sync + 'static,
    {
        use std::sync::Arc;
        let processor = Arc::new(processor);
        let mut futures = FuturesUnordered::new();
        let start_time = std::time::Instant::now();
        let mut processed = 0;

        while let Some(event) = self.pop_next_event() {
            let permit = self.semaphore.clone().acquire_owned().await.unwrap();
            let event_clone = event.clone();
            let processor = Arc::clone(&processor);

            futures.push(tokio::spawn(async move {
                let result = (processor)(event_clone);
                drop(permit);
                result
            }));

            processed += 1;
        }

        while let Some(result) = futures.next().await {
            result.unwrap()?;
        }

        // Update statistics
        let mut stats = self.stats.write();
        let elapsed = start_time.elapsed();
        stats.events_per_second = processed as f64 / elapsed.as_secs_f64();
        stats.avg_processing_time_ms = elapsed.as_secs_f64() * 1000.0 / processed as f64;

        Ok(())
    }

    pub fn get_stats(&self) -> EventStats {
        self.stats.read().clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: Uuid,
    pub time: f64,
    pub timestamp: DateTime<Utc>,
    pub events: EventStats,
    pub state: BTreeMap<String, serde_json::Value>,
    pub pending_events: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWarpState {
    pub enabled: bool,
    pub rollback_window: f64,
    pub anti_messages: HashMap<Uuid, Event>,
    pub local_virtual_time: f64,
    pub global_virtual_time: f64,
}

impl Default for TimeWarpState {
    fn default() -> Self {
        Self {
            enabled: false,
            rollback_window: 10.0,
            anti_messages: HashMap::new(),
            local_virtual_time: 0.0,
            global_virtual_time: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct DiscreteEventSimulator {
    current_time: Arc<RwLock<f64>>,
    event_queue: EventQueue,
    config: SimulationConfig,
    state_history: Arc<RwLock<BTreeMap<OrderedFloat<f64>, serde_json::Value>>>,
    checkpoints: Arc<RwLock<BTreeMap<OrderedFloat<f64>, Checkpoint>>>,
    time_warp_state: Arc<RwLock<TimeWarpState>>,
    rollback_count: Arc<RwLock<u64>>,
}

impl DiscreteEventSimulator {
    pub fn new(config: SimulationConfig) -> Self {
        Self {
            current_time: Arc::new(RwLock::new(0.0)),
            event_queue: EventQueue::new(config.max_concurrent_events),
            config,
            state_history: Arc::new(RwLock::new(
                BTreeMap::<OrderedFloat<f64>, serde_json::Value>::new(),
            )),
            checkpoints: Arc::new(RwLock::new(BTreeMap::new())),
            time_warp_state: Arc::new(RwLock::new(TimeWarpState::default())),
            rollback_count: Arc::new(RwLock::new(0)),
        }
    }

    pub fn enable_time_warp(&self, rollback_window: f64) {
        let mut time_warp = self.time_warp_state.write();
        time_warp.enabled = true;
        time_warp.rollback_window = rollback_window;
        info!(
            "Time warp enabled with rollback window of {}",
            rollback_window
        );
    }

    pub fn disable_time_warp(&self) {
        let mut time_warp = self.time_warp_state.write();
        time_warp.enabled = false;
        info!("Time warp disabled");
    }

    pub fn current_time(&self) -> f64 {
        *self.current_time.read()
    }

    pub async fn run(&self) -> Result<(), SimulationError> {
        let start_time = std::time::Instant::now();
        let mut events_processed = 0;
        let mut last_checkpoint = start_time;

        while let Some(event) = self.event_queue.peek_next_event() {
            if event.time > self.config.max_time {
                break;
            }

            if let Some(max_events) = self.config.max_events {
                if events_processed >= max_events {
                    break;
                }
            }

            if self.config.parallel_execution {
                // Clone the pieces required by the closure so that it can be 'static
                let state_history = self.state_history.clone();
                self.event_queue
                    .process_parallel_events(move |event| {
                        // Directly process the event without borrowing `self`.
                        match &event.data {
                            EventData::AgentAction {
                                agent_id,
                                action,
                                parameters: _,
                            } => {
                                info!("Processing agent action: {} for agent {}", action, agent_id);
                            }
                            EventData::StateChange {
                                entity_id,
                                state,
                                value,
                            } => {
                                let mut history = state_history.write();
                                history.insert(OrderedFloat(event.time), value.clone());
                                info!("State change for entity {}: {}", entity_id, state);
                            }
                            EventData::Interaction {
                                source_id,
                                target_id,
                                interaction_type,
                                data: _,
                            } => {
                                info!(
                                    "Processing interaction {} between {} and {}",
                                    interaction_type, source_id, target_id
                                );
                            }
                            EventData::SystemEvent {
                                event_type,
                                data: _,
                            } => {
                                info!("Processing system event: {}", event_type);
                            }
                        }
                        Ok(())
                    })
                    .await?;
            } else {
                if let Some(event) = self.event_queue.pop_next_event() {
                    self.process_event(event)?;
                }
            }

            events_processed += 1;

            // Update simulation time
            *self.current_time.write() = event.time;

            // Checkpoint on the wall-clock interval, measured since the last
            // checkpoint (not since start) so we don't checkpoint on every
            // iteration once the first interval has elapsed.
            if let Some(interval) = self.config.checkpoint_interval {
                if last_checkpoint.elapsed() >= interval {
                    self.create_checkpoint()?;
                    last_checkpoint = std::time::Instant::now();
                }
            }
        }

        // When checkpointing is enabled, always leave at least one checkpoint as a
        // resume point: short runs may finish before the first interval elapses.
        if self.config.checkpoint_interval.is_some() && events_processed > 0 {
            self.create_checkpoint()?;
        }

        Ok(())
    }

    fn process_event(&self, event: Event) -> Result<(), SimulationError> {
        match &event.data {
            EventData::AgentAction {
                agent_id, action, ..
            } => {
                // Process agent action
                info!("Processing agent action: {} for agent {}", action, agent_id);
            }
            EventData::StateChange {
                entity_id,
                state,
                value,
            } => {
                // Update state history
                let mut history = self.state_history.write();
                history.insert(OrderedFloat(event.time), value.clone());
                info!("State change for entity {}: {}", entity_id, state);
            }
            EventData::Interaction {
                source_id,
                target_id,
                interaction_type,
                ..
            } => {
                // Handle interaction between entities
                info!(
                    "Processing interaction {} between {} and {}",
                    interaction_type, source_id, target_id
                );
            }
            EventData::SystemEvent { event_type, .. } => {
                // Handle system-level events
                info!("Processing system event: {}", event_type);
            }
        }

        Ok(())
    }

    fn create_checkpoint(&self) -> Result<(), SimulationError> {
        let time = self.current_time();

        // Collect all pending events
        let pending_events: Vec<Event> = {
            let queue = self.event_queue.queue.read();
            let events = &self.event_queue.events;
            queue
                .iter()
                .filter_map(|(id, _)| events.get(id).map(|e| e.clone()))
                .collect()
        };

        let checkpoint_id = Uuid::new_v4();
        let checkpoint = Checkpoint {
            id: checkpoint_id,
            time,
            timestamp: Utc::now(),
            events: self.event_queue.get_stats(),
            state: self
                .state_history
                .read()
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            pending_events,
        };

        let mut checkpoints = self.checkpoints.write();
        checkpoints.insert(OrderedFloat(time), checkpoint);

        // Keep only recent checkpoints (last 10)
        if checkpoints.len() > 10 {
            if let Some(&oldest_time) = checkpoints.keys().next() {
                checkpoints.remove(&oldest_time);
            }
        }

        info!("Created checkpoint {} at time {}", checkpoint_id, time);
        Ok(())
    }

    pub fn rollback_to_time(&self, target_time: f64) -> Result<(), SimulationError> {
        if target_time > self.current_time() {
            return Err(SimulationError::state_error(
                StateErrorKind::RollbackError,
                format!("Cannot rollback to future time: {}", target_time),
            ));
        }

        info!(
            "Rolling back from {} to {}",
            self.current_time(),
            target_time
        );

        // Find the checkpoint at or before target_time
        let checkpoint = {
            let checkpoints = self.checkpoints.read();
            checkpoints
                .range(..=OrderedFloat(target_time))
                .next_back()
                .map(|(_, cp)| cp.clone())
                .ok_or_else(|| {
                    SimulationError::state_error(
                        StateErrorKind::CheckpointError,
                        format!("No checkpoint found at or before time {}", target_time),
                    )
                })?
        };

        // Restore state from checkpoint
        *self.current_time.write() = checkpoint.time;

        // Restore state history
        {
            let mut history = self.state_history.write();
            history.clear();
            for (key_str, value) in &checkpoint.state {
                if let Ok(time) = key_str.parse::<f64>() {
                    history.insert(OrderedFloat(time), value.clone());
                }
            }
        }

        // Clear and restore event queue
        {
            let mut queue = self.event_queue.queue.write();
            queue.clear();
            self.event_queue.events.clear();

            for event in checkpoint.pending_events {
                if event.time >= target_time {
                    self.event_queue.schedule_event(event)?;
                }
            }
        }

        // Update rollback count
        *self.rollback_count.write() += 1;

        info!("Rollback completed to time {}", target_time);
        Ok(())
    }

    pub fn rollback_to_checkpoint(&self, checkpoint_id: Uuid) -> Result<(), SimulationError> {
        let checkpoint = {
            let checkpoints = self.checkpoints.read();
            checkpoints
                .values()
                .find(|cp| cp.id == checkpoint_id)
                .cloned()
                .ok_or_else(|| {
                    SimulationError::state_error(
                        StateErrorKind::CheckpointError,
                        format!("Checkpoint {} not found", checkpoint_id),
                    )
                })?
        };

        self.rollback_to_time(checkpoint.time)
    }

    pub fn list_checkpoints(&self) -> Vec<Checkpoint> {
        let checkpoints = self.checkpoints.read();
        checkpoints.values().cloned().collect()
    }

    pub fn get_rollback_count(&self) -> u64 {
        *self.rollback_count.read()
    }

    pub fn process_with_time_warp(&self, event: Event) -> Result<(), SimulationError> {
        let mut time_warp = self.time_warp_state.write();

        if !time_warp.enabled {
            return self.process_event(event);
        }

        // Check if event time is in the past (causality violation)
        if event.time < time_warp.local_virtual_time {
            warn!(
                "Causality violation detected: event at {} < LVT {}",
                event.time, time_warp.local_virtual_time
            );

            // Send anti-message if this was caused by a previous event
            if let Some(previous_event_id) = event.causality_chain.last() {
                let anti_event = Event {
                    id: Uuid::new_v4(),
                    time: event.time,
                    priority: Priority::Critical,
                    data: EventData::SystemEvent {
                        event_type: "anti_message".to_string(),
                        data: serde_json::json!({
                            "original_event": previous_event_id,
                            "canceled_event": event.id,
                        }),
                    },
                    metadata: Some(serde_json::json!({"is_anti_message": true})),
                    created_at: Utc::now(),
                    dependencies: vec![],
                    causality_chain: event.causality_chain.clone(),
                    retry_count: 0,
                    max_retries: 0,
                    timeout: None,
                    rollback_handler: event.rollback_handler.clone(),
                };

                time_warp.anti_messages.insert(event.id, anti_event);
            }

            // Rollback to before the violating event
            let rollback_time = (event.time - time_warp.rollback_window).max(0.0);
            drop(time_warp); // Release lock before rollback
            self.rollback_to_time(rollback_time)?;

            // Re-acquire lock
            let mut time_warp = self.time_warp_state.write();
            time_warp.local_virtual_time = rollback_time;

            return Ok(());
        }

        // Update local virtual time
        time_warp.local_virtual_time = event.time;

        // Check for anti-message cancellation
        if let Some(_anti_event) = time_warp.anti_messages.remove(&event.id) {
            info!("Event {} canceled by anti-message", event.id);
            return Ok(());
        }

        // Process event normally
        drop(time_warp); // Release lock before processing
        self.process_event(event)
    }

    pub fn get_time_warp_state(&self) -> TimeWarpState {
        self.time_warp_state.read().clone()
    }

    pub fn schedule_event(&self, event: Event) -> Result<(), SimulationError> {
        if event.time < self.current_time() {
            return Err(SimulationError::time_error(
                TimeErrorKind::CausalityViolation,
                "Cannot schedule events in the past",
            ));
        }
        self.event_queue.schedule_event(event)
    }

    pub fn cancel_event(&self, event_id: Uuid) -> Result<(), SimulationError> {
        self.event_queue.cancel_event(event_id)
    }

    pub fn get_stats(&self) -> EventStats {
        self.event_queue.get_stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_scheduling() {
        let config = SimulationConfig {
            max_time: 100.0,
            max_events: None,
            parallel_execution: false,
            max_concurrent_events: 10,
            checkpoint_interval: None,
            metrics_enabled: true,
        };

        let simulator = DiscreteEventSimulator::new(config);

        // Schedule some test events
        let event1 = Event {
            id: Uuid::new_v4(),
            time: 1.0,
            priority: Priority::High,
            data: EventData::SystemEvent {
                event_type: "test".into(),
                data: serde_json::json!({"test": true}),
            },
            metadata: None,
            created_at: Utc::now(),
            dependencies: Vec::new(),
            causality_chain: Vec::new(),
            retry_count: 0,
            max_retries: 3,
            timeout: None,
            rollback_handler: None,
        };

        let event2 = Event {
            id: Uuid::new_v4(),
            time: 2.0,
            priority: Priority::Normal,
            data: EventData::SystemEvent {
                event_type: "test2".into(),
                data: serde_json::json!({"test": false}),
            },
            metadata: None,
            created_at: Utc::now(),
            dependencies: Vec::new(),
            causality_chain: Vec::new(),
            retry_count: 0,
            max_retries: 3,
            timeout: None,
            rollback_handler: None,
        };

        simulator.schedule_event(event1).unwrap();
        simulator.schedule_event(event2).unwrap();

        // Run simulation
        simulator.run().await.unwrap();

        // Check results
        let stats = simulator.get_stats();
        assert_eq!(stats.total_events, 2);
        assert_eq!(stats.processed_events, 2);
        assert_eq!(stats.pending_events, 0);
    }

    #[tokio::test]
    async fn test_parallel_execution() {
        let config = SimulationConfig {
            max_time: 100.0,
            max_events: None,
            parallel_execution: true,
            max_concurrent_events: 4,
            checkpoint_interval: None,
            metrics_enabled: true,
        };

        let simulator = DiscreteEventSimulator::new(config);

        // Schedule multiple events at same time
        for i in 0..10 {
            let event = Event {
                id: Uuid::new_v4(),
                time: 1.0,
                priority: Priority::Normal,
                data: EventData::SystemEvent {
                    event_type: format!("test{}", i),
                    data: serde_json::json!({"index": i}),
                },
                metadata: None,
                created_at: Utc::now(),
                dependencies: Vec::new(),
                causality_chain: Vec::new(),
                retry_count: 0,
                max_retries: 3,
                timeout: None,
                rollback_handler: None,
            };
            simulator.schedule_event(event).unwrap();
        }

        // Run simulation
        simulator.run().await.unwrap();

        // Check results
        let stats = simulator.get_stats();
        assert_eq!(stats.total_events, 10);
        assert_eq!(stats.processed_events, 10);
        assert!(stats.events_per_second > 0.0);
    }

    #[tokio::test]
    async fn test_checkpointing() {
        let config = SimulationConfig {
            max_time: 100.0,
            max_events: Some(5),
            parallel_execution: false,
            max_concurrent_events: 1,
            checkpoint_interval: Some(StdDuration::from_millis(100)),
            metrics_enabled: true,
        };

        let simulator = DiscreteEventSimulator::new(config);

        // Schedule events
        for i in 0..5 {
            let event = Event {
                id: Uuid::new_v4(),
                time: (i + 1) as f64,
                priority: Priority::Normal,
                data: EventData::StateChange {
                    entity_id: Uuid::new_v4(),
                    state: format!("state_{}", i),
                    value: serde_json::json!({"value": i}),
                },
                metadata: None,
                created_at: Utc::now(),
                dependencies: Vec::new(),
                causality_chain: Vec::new(),
                retry_count: 0,
                max_retries: 3,
                timeout: None,
                rollback_handler: None,
            };
            simulator.schedule_event(event).unwrap();
        }

        // Run simulation
        simulator.run().await.unwrap();

        // Check that checkpoints were created
        let checkpoints = simulator.list_checkpoints();
        assert!(!checkpoints.is_empty());

        // Verify checkpoint contains state
        let last_checkpoint = checkpoints.last().unwrap();
        assert!(!last_checkpoint.state.is_empty());
    }

    #[tokio::test]
    async fn test_rollback() {
        let config = SimulationConfig {
            max_time: 100.0,
            max_events: None,
            parallel_execution: false,
            max_concurrent_events: 1,
            checkpoint_interval: None,
            metrics_enabled: true,
        };

        let simulator = DiscreteEventSimulator::new(config);

        // Schedule events
        for i in 0..5 {
            let event = Event {
                id: Uuid::new_v4(),
                time: (i + 1) as f64,
                priority: Priority::Normal,
                data: EventData::StateChange {
                    entity_id: Uuid::new_v4(),
                    state: format!("state_{}", i),
                    value: serde_json::json!({"value": i}),
                },
                metadata: None,
                created_at: Utc::now(),
                dependencies: Vec::new(),
                causality_chain: Vec::new(),
                retry_count: 0,
                max_retries: 3,
                timeout: None,
                rollback_handler: None,
            };
            simulator.schedule_event(event).unwrap();
        }

        // Run partially
        simulator.run().await.unwrap();

        // Create a checkpoint at current time
        simulator.create_checkpoint().unwrap();
        let checkpoints = simulator.list_checkpoints();
        assert_eq!(checkpoints.len(), 1);

        // Rollback
        let checkpoint_id = checkpoints[0].id;
        simulator.rollback_to_checkpoint(checkpoint_id).unwrap();

        assert_eq!(simulator.get_rollback_count(), 1);
    }

    #[tokio::test]
    async fn test_time_warp() {
        let config = SimulationConfig {
            max_time: 100.0,
            max_events: None,
            parallel_execution: false,
            max_concurrent_events: 1,
            checkpoint_interval: None,
            metrics_enabled: true,
        };

        let simulator = DiscreteEventSimulator::new(config);
        simulator.enable_time_warp(5.0);

        // Schedule events in order
        for i in 0..3 {
            let event = Event {
                id: Uuid::new_v4(),
                time: (i + 1) as f64 * 2.0,
                priority: Priority::Normal,
                data: EventData::StateChange {
                    entity_id: Uuid::new_v4(),
                    state: format!("state_{}", i),
                    value: serde_json::json!({"value": i}),
                },
                metadata: None,
                created_at: Utc::now(),
                dependencies: Vec::new(),
                causality_chain: Vec::new(),
                retry_count: 0,
                max_retries: 3,
                timeout: None,
                rollback_handler: None,
            };
            simulator.schedule_event(event).unwrap();
        }

        // Verify time warp is enabled
        let time_warp_state = simulator.get_time_warp_state();
        assert!(time_warp_state.enabled);
        assert_eq!(time_warp_state.rollback_window, 5.0);
    }
}
