use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for reinforcement learning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLConfig {
    pub learning_rate: f64,
    pub discount_factor: f64,
    pub epsilon: f64,
    pub batch_size: usize,
}

/// Represents an environment state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentState {
    pub state: Vec<f64>,
    pub reward: f64,
    pub done: bool,
    pub info: HashMap<String, String>,
}

/// Represents an action in the environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentAction {
    pub action: Vec<f64>,
    pub action_type: String,
}

/// Represents an experience tuple for RL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub state: Vec<f64>,
    pub action: Vec<f64>,
    pub reward: f64,
    pub next_state: Vec<f64>,
    pub done: bool,
}

/// Environment interface
pub trait Environment: Send + Sync {
    fn reset(&mut self) -> Result<EnvironmentState>;
    fn step(&mut self, action: EnvironmentAction) -> Result<(EnvironmentState, f64, bool)>;
    fn get_state(&self) -> Result<EnvironmentState>;
}

/// Policy interface
pub trait Policy: Send + Sync {
    fn select_action(&self, state: &EnvironmentState) -> Result<EnvironmentAction>;
    fn update(&mut self, experience: &Experience) -> Result<()>;
}

/// Value function interface
pub trait Value: Send + Sync {
    fn evaluate(&self, state: &EnvironmentState) -> Result<f64>;
    fn update(&mut self, state: &EnvironmentState, target: f64) -> Result<()>;
}

/// Trajectory for RL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trajectory {
    pub states: Vec<EnvironmentState>,
    pub actions: Vec<EnvironmentAction>,
    pub rewards: Vec<f64>,
    pub total_reward: f64,
}

/// Represents a replay buffer for storing experiences
pub struct ReplayBuffer {
    experiences: Vec<Experience>,
    max_size: usize,
}

impl ReplayBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            experiences: Vec::new(),
            max_size,
        }
    }

    pub fn add(&mut self, experience: Experience) {
        if self.experiences.len() >= self.max_size {
            self.experiences.remove(0);
        }
        self.experiences.push(experience);
    }

    pub fn sample(&self, batch_size: usize) -> Vec<Experience> {
        // TODO: Implement proper sampling
        self.experiences.iter().take(batch_size).cloned().collect()
    }
}

// Remove this line since these are already defined in the module
