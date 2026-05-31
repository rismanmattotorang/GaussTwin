//! Model management for GaussTwin simulations
//!
//! This module provides the core Model trait and implementations for
//! managing simulation state, configuration, and execution.

use crate::{
    agent::{Agent, AgentContext, AgentId, AgentSet},
    error::Result,
    event::EventQueue,
    scheduler::{Scheduler, SchedulerKind},
    space::{Bounds, Space, VecN},
    time::{Duration, SimTime, TimeStep},
};
use async_trait::async_trait;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use uuid::Uuid;

/// Unique identifier for models
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelId(Uuid);

impl ModelId {
    /// Create a new random model ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ModelId {
    fn default() -> Self {
        Self::new()
    }
}

/// Model configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model name
    pub name: String,
    /// Model description
    pub description: Option<String>,
    /// Initial simulation time
    pub start_time: SimTime,
    /// End simulation time
    pub end_time: SimTime,
    /// Time step configuration
    pub time_step: TimeStep,
    /// Scheduler type to use
    pub scheduler_kind: SchedulerKind,
    /// Maximum number of agents
    pub max_agents: Option<usize>,
    /// Random seed for reproducibility
    pub seed: Option<u64>,
    /// Custom parameters
    pub parameters: HashMap<String, serde_json::Value>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            name: "Untitled Model".to_string(),
            description: None,
            start_time: SimTime::zero(),
            end_time: SimTime::new(100.0),
            time_step: TimeStep::fixed(1.0).unwrap(),
            scheduler_kind: SchedulerKind::Random,
            max_agents: None,
            seed: None,
            parameters: HashMap::new(),
        }
    }
}

impl ModelConfig {
    /// Create a new model configuration
    pub fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    /// Set description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Set time range
    pub fn with_time_range(mut self, start: SimTime, end: SimTime) -> Self {
        self.start_time = start;
        self.end_time = end;
        self
    }

    /// Set time step
    pub fn with_time_step(mut self, time_step: TimeStep) -> Self {
        self.time_step = time_step;
        self
    }

    /// Set scheduler
    pub fn with_scheduler(mut self, scheduler_kind: SchedulerKind) -> Self {
        self.scheduler_kind = scheduler_kind;
        self
    }

    /// Set maximum agents
    pub fn with_max_agents(mut self, max_agents: usize) -> Self {
        self.max_agents = Some(max_agents);
        self
    }

    /// Set random seed
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Add a parameter
    pub fn with_parameter(mut self, key: String, value: serde_json::Value) -> Self {
        self.parameters.insert(key, value);
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        use crate::error::GaussTwinError;

        if self.start_time >= self.end_time {
            return Err(GaussTwinError::InvalidModelConfig(
                "Start time must be before end time".to_string(),
            ));
        }

        if let Some(max_agents) = self.max_agents {
            if max_agents == 0 {
                return Err(GaussTwinError::InvalidModelConfig(
                    "Maximum agents must be greater than 0".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Model execution state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModelState {
    /// Model is created but not initialized
    Created,
    /// Model is initialized and ready to run
    Initialized,
    /// Model is currently running
    Running,
    /// Model execution is paused
    Paused,
    /// Model execution is completed
    Completed,
    /// Model execution failed
    Failed(String),
}

/// Model performance metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelMetrics {
    /// Current simulation time
    pub current_time: SimTime,
    /// Total simulation steps executed
    pub steps_executed: u64,
    /// Total wall-clock time elapsed
    pub wall_time_elapsed: Duration,
    /// Steps per second performance
    pub steps_per_second: f64,
    /// Current number of agents
    pub agent_count: usize,
    /// Total events processed
    pub events_processed: u64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Custom metrics
    pub custom_metrics: HashMap<String, f64>,
}

/// Core trait for simulation models
///
/// This trait defines the interface for simulation models that manage agents,
/// scheduling, and execution flow.
pub trait Model<S: crate::agent::AgentState>: Send + Sync {
    /// Get model ID
    fn id(&self) -> ModelId;

    /// Get model configuration
    fn config(&self) -> &ModelConfig;

    /// Get current model state
    fn state(&self) -> ModelState;

    /// Get current simulation time
    fn current_time(&self) -> SimTime;

    /// Initialize the model
    fn initialize(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Execute one simulation step
    fn step(&mut self) -> impl std::future::Future<Output = Result<bool>> + Send;

    /// Run the simulation for specified duration
    fn run(
        &mut self,
        duration: Option<Duration>,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Pause the simulation
    fn pause(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Resume the simulation
    fn resume(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Stop the simulation
    fn stop(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Reset the simulation to initial state
    fn reset(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Get model metrics
    fn metrics(&self) -> ModelMetrics;

    /// Get agent by ID
    fn get_agent(&self, id: AgentId) -> Option<&dyn Agent<State = S>>;

    /// Get mutable agent by ID
    fn get_agent_mut(&mut self, id: AgentId) -> Option<&mut dyn Agent<State = S>>;

    /// Add an agent to the model
    fn add_agent(
        &mut self,
        agent: Box<dyn Agent<State = S>>,
    ) -> impl std::future::Future<Output = Result<AgentId>> + Send;

    /// Remove an agent from the model
    fn remove_agent(&mut self, id: AgentId)
        -> impl std::future::Future<Output = Result<()>> + Send;

    /// Get all agent IDs
    fn agent_ids(&self) -> Vec<AgentId>;

    /// Get model snapshot for serialization
    fn snapshot(&self) -> Result<serde_json::Value>;

    /// Restore model from snapshot
    fn restore(
        &mut self,
        snapshot: serde_json::Value,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Standard model implementation
pub struct StandardModel<S: crate::agent::AgentState> {
    id: ModelId,
    config: ModelConfig,
    state: ModelState,
    current_time: SimTime,
    agents: AgentSet<S>,
    agent_registry: HashMap<String, Box<dyn Fn() -> Box<dyn Agent<State = S>> + Send + Sync>>,
    space: Box<dyn Space>,
    scheduler: Box<dyn Scheduler<S>>,
    event_queue: EventQueue,
    // metrics_collector: MetricsCollector,  // TODO: Implement metrics
    start_wall_time: Option<std::time::Instant>,
    step_count: u64,
}

impl<S: crate::agent::AgentState> StandardModel<S> {
    /// Create a new standard model
    pub fn new(config: ModelConfig) -> Result<Self> {
        // Build the random scheduler from the configured seed when one is set, so
        // `ModelConfig::with_seed` actually makes runs reproducible. Without a seed,
        // fall back to a randomly-seeded scheduler.
        let make_random = || match config.seed {
            Some(seed) => crate::scheduler::RandomScheduler::new(seed),
            None => crate::scheduler::RandomScheduler::new_random(),
        };
        let scheduler: Box<dyn Scheduler<S>> = match config.scheduler_kind {
            SchedulerKind::Random => Box::new(make_random()),
            SchedulerKind::Sequential => Box::new(crate::scheduler::SequentialScheduler::new()),
            SchedulerKind::Parallel => Box::new(crate::scheduler::ParallelScheduler::auto_batch()),
            SchedulerKind::Priority => Box::new(crate::scheduler::PriorityScheduler::new()),
            SchedulerKind::EventBased => Box::new(crate::scheduler::EventScheduler::new()),
            SchedulerKind::Custom(_) => Box::new(make_random()), // Default fallback
        };

        Ok(Self {
            id: ModelId::new(),
            config,
            state: ModelState::Created,
            current_time: SimTime::zero(),
            agents: AgentSet::new(),
            agent_registry: HashMap::new(),
            space: create_default_space(),
            scheduler,
            event_queue: EventQueue::new(),
            // metrics_collector: MetricsCollector::new(),  // TODO: Implement metrics
            start_wall_time: None,
            step_count: 0,
        })
    }

    /// Set a custom space implementation
    pub fn with_space(mut self, space: Box<dyn Space>) -> Self {
        self.space = space;
        self
    }

    /// Set a custom scheduler implementation
    pub fn with_scheduler(mut self, scheduler: Box<dyn Scheduler<S>>) -> Self {
        self.scheduler = scheduler;
        self
    }

    /// Register an agent type with the model
    pub fn register_agent_type(
        &mut self,
        name: String,
        factory: Box<dyn Fn() -> Box<dyn Agent<State = S>> + Send + Sync>,
    ) {
        self.agent_registry.insert(name, factory);
    }
}

pub fn create_default_space() -> Box<dyn Space> {
    Box::new(crate::space::continuous::HashMapSpace::new(Bounds {
        min: Vector3::new(-1000.0, -1000.0, -1000.0),
        max: Vector3::new(1000.0, 1000.0, 1000.0),
    }))
}

impl<S: crate::agent::AgentState + Default> Model<S> for StandardModel<S> {
    fn id(&self) -> ModelId {
        self.id
    }

    fn config(&self) -> &ModelConfig {
        &self.config
    }

    fn state(&self) -> ModelState {
        self.state.clone()
    }

    fn current_time(&self) -> SimTime {
        self.current_time
    }

    fn initialize(&mut self) -> impl std::future::Future<Output = Result<()>> + Send {
        async move {
            use crate::error::GaussTwinError;

            if self.state != ModelState::Created {
                return Err(GaussTwinError::ModelExecutionFailed(
                    "Model already initialized".to_string(),
                ));
            }

            // Set initial time
            self.current_time = self.config.start_time;

            // Initialize scheduler with current agents
            let agent_ids = self.agents.agent_ids();
            self.scheduler.initialize(&agent_ids)?;

            // Initialize event queue
            self.event_queue.set_current_time(self.current_time);

            // Initialize metrics collection
            // self.metrics_collector.start_collection(self.current_time);  // TODO: Implement metrics

            self.state = ModelState::Initialized;
            tracing::info!("Model {} initialized", self.config.name);

            Ok(())
        }
    }

    fn step(&mut self) -> impl std::future::Future<Output = Result<bool>> + Send {
        async move {
            use crate::error::GaussTwinError;

            match self.state {
                ModelState::Initialized | ModelState::Running => {}
                ModelState::Paused => return Ok(false),
                ModelState::Completed => return Ok(false),
                _ => {
                    return Err(GaussTwinError::ModelExecutionFailed(format!(
                        "Cannot step model in state {:?}",
                        self.state
                    )))
                }
            }

            // Set state to running
            self.state = ModelState::Running;

            if self.start_wall_time.is_none() {
                self.start_wall_time = Some(std::time::Instant::now());
            }

            // Process events first
            while let Some(event) = self.event_queue.next_event() {
                // Handle event (simplified - would dispatch to appropriate handlers)
                tracing::debug!("Processing event {:?}", event.id);
            }

            // Reset scheduler for this step
            self.scheduler.reset_step()?;

            // Execute agents in batches
            while self.scheduler.has_next() {
                let batch = self.scheduler.next_batch(self.current_time)?;

                // Process each agent in the batch
                for agent_id in batch {
                    if let Some(_agent) = self.agents.get_agent_mut(&agent_id) {
                        let _context: AgentContext<S> = AgentContext {
                            agent_id: agent_id,
                            current_time: self.current_time,
                            time_step: self.config.time_step.duration().as_secs(),
                            shared_state: Default::default(),
                            messages: vec![],
                        };

                        // This would need to be properly implemented with async context
                        // For now, we just count the step
                    }
                }
            }

            // Advance simulation time
            self.current_time = self.current_time + self.config.time_step.duration();
            self.step_count += 1;

            // Update event queue time
            self.event_queue.set_current_time(self.current_time);

            // Collect metrics
            // self.metrics_collector.record_step(self.current_time);  // TODO: Implement metrics

            // Check if simulation should end
            let should_continue = self.current_time < self.config.end_time;
            if !should_continue {
                self.state = ModelState::Completed;
                tracing::info!("Model {} completed", self.config.name);
            }

            Ok(should_continue)
        }
    }

    fn run(
        &mut self,
        duration: Option<Duration>,
    ) -> impl std::future::Future<Output = Result<()>> + Send {
        let end_time = if let Some(duration) = duration {
            self.current_time + duration
        } else {
            self.config.end_time
        };

        async move {
            while self.current_time < end_time && self.state() == ModelState::Running {
                let should_continue = self.step().await?;
                if !should_continue {
                    break;
                }
            }

            Ok(())
        }
    }

    fn pause(&mut self) -> impl std::future::Future<Output = Result<()>> + Send {
        async move {
            if self.state == ModelState::Running {
                self.state = ModelState::Paused;
                tracing::info!("Model {} paused", self.config.name);
            }
            Ok(())
        }
    }

    fn resume(&mut self) -> impl std::future::Future<Output = Result<()>> + Send {
        async move {
            if self.state == ModelState::Paused {
                self.state = ModelState::Running;
                tracing::info!("Model {} resumed", self.config.name);
            }
            Ok(())
        }
    }

    fn stop(&mut self) -> impl std::future::Future<Output = Result<()>> + Send {
        async move {
            self.state = ModelState::Completed;
            tracing::info!("Model {} stopped", self.config.name);
            Ok(())
        }
    }

    fn reset(&mut self) -> impl std::future::Future<Output = Result<()>> + Send {
        async move {
            self.current_time = self.config.start_time;
            self.step_count = 0;
            self.state = ModelState::Created;
            self.start_wall_time = None;

            // Reset components
            self.event_queue.clear();
            // self.metrics_collector.reset();  // TODO: Implement metrics

            tracing::info!("Model {} reset", self.config.name);
            Ok(())
        }
    }

    fn metrics(&self) -> ModelMetrics {
        let wall_time_elapsed = if let Some(start) = self.start_wall_time {
            Duration::from_secs(start.elapsed().as_secs_f64())
        } else {
            Duration::zero()
        };

        let steps_per_second = if wall_time_elapsed.is_positive() {
            self.step_count as f64 / wall_time_elapsed.as_secs()
        } else {
            0.0
        };

        ModelMetrics {
            current_time: self.current_time,
            steps_executed: self.step_count,
            wall_time_elapsed,
            steps_per_second,
            agent_count: self.agents.len(),
            events_processed: 0, // TODO: Track events
            memory_usage: 0,     // TODO: Track memory
            custom_metrics: HashMap::new(),
        }
    }

    fn get_agent(&self, id: AgentId) -> Option<&dyn Agent<State = S>> {
        self.agents.get_agent(&id)
    }

    fn get_agent_mut(&mut self, id: AgentId) -> Option<&mut dyn Agent<State = S>> {
        self.agents.get_agent_mut(&id)
    }

    fn add_agent(
        &mut self,
        agent: Box<dyn Agent<State = S>>,
    ) -> impl std::future::Future<Output = Result<AgentId>> + Send {
        async move {
            // Check max agents limit
            if let Some(max_agents) = self.config.max_agents {
                if self.agents.len() >= max_agents {
                    return Err(crate::error::GaussTwinError::ResourceExhausted(format!(
                        "Maximum number of agents ({}) reached",
                        max_agents
                    )));
                }
            }

            let agent_id = agent.id();

            // Add to scheduler
            self.scheduler.add_agent(agent_id)?;

            // Add to agent set
            self.agents.add_agent(agent);

            Ok(agent_id)
        }
    }

    fn remove_agent(
        &mut self,
        id: AgentId,
    ) -> impl std::future::Future<Output = Result<()>> + Send {
        async move {
            // Remove from scheduler
            self.scheduler.remove_agent(id)?;

            // Remove from agent set
            if let Some(_agent) = self.agents.remove_agent(id) {
                // Agent was successfully removed
            }

            Ok(())
        }
    }

    fn agent_ids(&self) -> Vec<AgentId> {
        self.agents.agent_ids()
    }

    fn snapshot(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "model_id": self.id.0.to_string(),
            "config": self.config,
            "state": self.state,
            "current_time": self.current_time,
            "step_count": self.step_count,
            "agent_count": self.agents.len(),
            // TODO: Add more snapshot data
        }))
    }

    fn restore(
        &mut self,
        _snapshot: serde_json::Value,
    ) -> impl std::future::Future<Output = Result<()>> + Send {
        async move {
            // TODO: Implement snapshot restoration
            Err(crate::error::GaussTwinError::NotImplemented(
                "Snapshot restoration not yet implemented".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
    struct TestState;

    impl crate::agent::AgentState for TestState {
        fn position(&self) -> Option<crate::space::VecN> {
            None
        }

        fn set_position(&mut self, _position: crate::space::VecN) {
            // Do nothing for test
        }

        fn properties(&self) -> std::collections::HashMap<String, serde_json::Value> {
            std::collections::HashMap::new()
        }

        fn set_property(&mut self, _key: String, _value: serde_json::Value) {
            // Do nothing for test
        }
    }

    #[test]
    fn test_model_config() {
        let config = ModelConfig::new("Test Model".to_string())
            .with_time_range(SimTime::zero(), SimTime::new(10.0))
            .with_seed(42);

        assert_eq!(config.name, "Test Model");
        assert_eq!(config.end_time, SimTime::new(10.0));
        assert_eq!(config.seed, Some(42));

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_model_config_validation() {
        let invalid_config = ModelConfig::new("Test".to_string())
            .with_time_range(SimTime::new(10.0), SimTime::new(5.0)); // Invalid range

        assert!(invalid_config.validate().is_err());
    }

    #[tokio::test]
    async fn test_standard_model() {
        let config = ModelConfig::new("Test Model".to_string());
        let mut model: StandardModel<TestState> = StandardModel::new(config).unwrap();

        assert_eq!(model.state(), ModelState::Created);

        model.initialize().await.unwrap();
        assert_eq!(model.state(), ModelState::Initialized);

        let metrics = model.metrics();
        assert_eq!(metrics.agent_count, 0);
        assert_eq!(metrics.steps_executed, 0);
    }
}
