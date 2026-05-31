use serde::{Deserialize, Serialize};

/// Configuration for the AI system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    pub model_config: ModelConfig,
    pub training_config: TrainingConfig,
    pub inference_config: InferenceConfig,
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model_type: String,
    pub input_size: usize,
    pub output_size: usize,
    pub hidden_layers: Vec<usize>,
}

/// Training configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    pub epochs: usize,
    pub batch_size: usize,
    pub learning_rate: f64,
    pub validation_split: f64,
}

/// Inference configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConfig {
    pub batch_size: usize,
    pub use_gpu: bool,
    pub precision: Precision,
}

/// Precision for inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Precision {
    F32,
    F16,
    I8,
}
