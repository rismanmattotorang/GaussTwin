use crate::core::{Action, State};
use crate::rl::Experience;
use crate::{Metrics, Result};
use async_trait::async_trait;
use std::sync::Arc;

/// Base trait for all AI agents
#[async_trait]
pub trait Agent: Send + Sync {
    /// Initialize the agent
    async fn init(&mut self) -> Result<()>;

    /// Select an action based on current state
    async fn act(&self, state: &State) -> Result<Action>;

    /// Update agent's policy based on experience
    async fn update(&mut self, experience: &Experience) -> Result<()>;

    /// Get agent's current performance metrics
    async fn get_metrics(&self) -> Result<Metrics>;

    /// Save agent's state
    async fn save(&self, path: &str) -> Result<()>;

    /// Load agent's state
    async fn load(&mut self, path: &str) -> Result<()>;
}

/// Environment interface
#[async_trait]
pub trait Environment: Send + Sync {
    /// Reset the environment
    async fn reset(&mut self) -> Result<State>;

    /// Step the environment with an action
    async fn step(&mut self, action: Action) -> Result<(State, f32, bool)>;

    /// Get environment state
    async fn get_state(&self) -> Result<State>;

    /// Set environment state
    async fn set_state(&mut self, state: State) -> Result<()>;

    /// Get environment information
    fn info(&self) -> EnvironmentInfo;
}

/// Model interface
#[async_trait]
pub trait Model: Send + Sync {
    type Input;
    type Output;
    type Config;

    /// Initialize the model
    async fn init(&mut self) -> Result<()>;

    /// Forward pass
    async fn forward(&self, input: &Self::Input) -> Result<Self::Output>;

    /// Training step
    async fn training_step(&mut self, batch: &Self::Input) -> Result<f32>;

    /// Validation step
    async fn validation_step(&self, batch: &Self::Input) -> Result<f32>;

    /// Get model parameters
    fn parameters(&self) -> Vec<Arc<dyn Parameter>>;

    /// Get model configuration
    fn config(&self) -> &Self::Config;
}

/// Parameter interface
pub trait Parameter: Send + Sync {
    /// Get parameter data
    fn data(&self) -> &[f32];

    /// Get parameter gradient
    fn gradient(&self) -> Option<&[f32]>;

    /// Update parameter
    fn update(&mut self, update: &[f32]) -> Result<()>;

    /// Zero gradient
    fn zero_grad(&mut self);
}

/// Optimizer interface
#[async_trait]
pub trait Optimizer: Send + Sync {
    /// Update parameters
    async fn step(&mut self, parameters: &[Arc<dyn Parameter>]) -> Result<()>;

    /// Zero all gradients
    fn zero_grad(&mut self);

    /// Get optimizer state
    fn state(&self) -> OptimizerState;

    /// Set optimizer state
    fn set_state(&mut self, state: OptimizerState) -> Result<()>;
}

/// Loss function interface
pub trait Loss: Send + Sync {
    /// Compute loss
    fn forward(&self, prediction: &[f32], target: &[f32]) -> Result<f32>;

    /// Compute gradient
    fn backward(&self, prediction: &[f32], target: &[f32]) -> Result<Vec<f32>>;
}

/// Metric interface
pub trait Metric: Send + Sync {
    /// Update metric with new data
    fn update(&mut self, prediction: &[f32], target: &[f32]);

    /// Get current metric value
    fn value(&self) -> f32;

    /// Reset metric
    fn reset(&mut self);
}

/// Data types
#[derive(Debug)]
pub struct EnvironmentInfo {
    pub state_space: Space,
    pub action_space: Space,
    pub max_steps: Option<usize>,
}

#[derive(Debug)]
pub struct OptimizerState {
    pub learning_rate: f32,
    pub iteration: usize,
    pub momentum: Option<f32>,
}

#[derive(Debug)]
pub enum Space {
    Discrete(usize),
    Continuous(Vec<(f32, f32)>),
    Binary(usize),
}
