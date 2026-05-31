use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents various metrics for AI system evaluation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metrics {
    pub accuracy: f64,
    pub loss: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub custom_metrics: HashMap<String, f64>,
}

/// Represents training metrics over time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingMetrics {
    pub epoch: usize,
    pub train_loss: f64,
    pub val_loss: f64,
    pub train_accuracy: f64,
    pub val_accuracy: f64,
    pub learning_rate: f64,
}

/// Represents inference metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceMetrics {
    pub latency_ms: f64,
    pub throughput: f64,
    pub memory_usage_mb: f64,
    pub gpu_utilization: f64,
}
