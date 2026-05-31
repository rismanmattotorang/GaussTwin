use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents the state of a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelState {
    pub model_id: String,
    pub parameters: HashMap<String, Vec<f64>>,
    pub metadata: HashMap<String, String>,
    pub version: u64,
}

/// Represents metrics for model evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetrics {
    pub accuracy: f64,
    pub loss: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
}

/// Represents the state of the AI system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIState {
    pub models: HashMap<String, ModelState>,
    pub metrics: HashMap<String, ModelMetrics>,
    pub timestamp: u64,
}
