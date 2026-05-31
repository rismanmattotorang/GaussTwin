use serde::{Deserialize, Serialize};

/// Represents the state of the AI system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub timestamp: u64,
    pub data: Vec<f64>,
}

/// Represents an action that can be taken by the AI system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub action_type: String,
    pub parameters: Vec<f64>,
}

/// Represents a configuration for the AI system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model_path: String,
    pub batch_size: usize,
    pub learning_rate: f64,
}
