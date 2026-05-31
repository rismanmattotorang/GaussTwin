use crate::{AIError, Result};
use std::sync::Arc;
use tch::{Device, Kind, Tensor};
use thiserror::Error;
use tokio::sync::RwLock;

/// ML-specific errors
#[derive(Error, Debug)]
pub enum MLError {
    #[error("Model architecture error: {0}")]
    ArchitectureError(String),
    #[error("Training error: {0}")]
    TrainingError(String),
    #[error("Inference error: {0}")]
    InferenceError(String),
    #[error("Data preprocessing error: {0}")]
    PreprocessingError(String),
}

pub mod data;
pub mod layers;
pub mod losses;
pub mod metrics;
pub mod models;
pub mod optimizers;
pub mod utils;

// Re-export commonly used types
pub use models::{GNNModel, ModelFactory, TransformerModel, VisionModel};

/// Model configuration for deep learning models
#[derive(Clone, Debug)]
pub struct ModelConfig {
    /// Model architecture type
    pub architecture: ModelArchitecture,
    /// Input dimensions
    pub input_dims: Vec<usize>,
    /// Output dimensions
    pub output_dims: Vec<usize>,
    /// Hidden layer configurations
    pub hidden_layers: Vec<LayerConfig>,
    /// Activation functions
    pub activations: Vec<Activation>,
    /// Dropout rates
    pub dropout_rates: Vec<f32>,
    /// Normalization layers
    pub normalizations: Vec<Normalization>,
    /// Learning rate
    pub learning_rate: f32,
    /// Batch size
    pub batch_size: usize,
    /// Device to run on (CPU/GPU)
    pub device: Device,
    /// Custom parameters
    pub custom_params: std::collections::HashMap<String, f32>,
    pub name: String,
}

/// Available model architectures
#[derive(Clone, Debug)]
pub enum ModelArchitecture {
    // Transformer-based architectures
    Transformer {
        num_layers: usize,
        num_heads: usize,
        d_model: usize,
        d_ff: usize,
        dropout: f32,
    },
    // Graph Neural Network architectures
    GNN {
        gnn_type: GNNType,
        aggregation: GraphAggregation,
        num_layers: usize,
        hidden_dims: Vec<usize>,
    },
    // Vision architectures
    Vision {
        backbone: VisionBackbone,
        pretrained: bool,
        freeze_backbone: bool,
    },
    // Time series architectures
    Temporal {
        temporal_type: TemporalType,
        hidden_dims: Vec<usize>,
        sequence_length: usize,
    },
}

/// GNN types based on latest research
#[derive(Clone, Debug)]
pub enum GNNType {
    // Message Passing Neural Networks
    MPNN {
        message_dims: Vec<usize>,
        update_dims: Vec<usize>,
    },
    // Graph Attention Networks
    GAT {
        num_heads: usize,
        concat_heads: bool,
    },
    // Graph Transformers
    GraphTransformer {
        num_heads: usize,
        edge_dims: usize,
    },
    // Temporal Graph Networks
    TGN {
        memory_dims: usize,
        temporal_dims: usize,
    },
    GCN,
    GraphSAGE,
    Temporal,
}

/// Graph aggregation methods
#[derive(Clone, Debug)]
pub enum GraphAggregation {
    Sum,
    Mean,
    Max,
    Attention { num_heads: usize, key_dims: usize },
}

/// Vision backbone architectures
#[derive(Clone, Debug)]
pub enum VisionBackbone {
    ResNet(usize),
    EfficientNet(String),
    ViT {
        patch_size: usize,
        num_heads: usize,
    },
    Swin {
        window_size: usize,
        shift_size: usize,
    },
}

/// Temporal architectures
#[derive(Clone, Debug)]
pub enum TemporalType {
    LSTM {
        hidden_size: usize,
        num_layers: usize,
    },
    GRU {
        hidden_size: usize,
        num_layers: usize,
    },
    Transformer {
        num_heads: usize,
        d_model: usize,
    },
    TCN {
        kernel_size: usize,
        dilation_base: usize,
    },
    TemporalConv,
}

/// Layer configuration
#[derive(Clone, Debug)]
pub struct LayerConfig {
    pub layer_type: LayerType,
    pub dims: Vec<usize>,
    pub activation: Option<Activation>,
    pub dropout: Option<f32>,
    pub normalization: Option<Normalization>,
}

/// Layer types
#[derive(Clone, Debug)]
pub enum LayerType {
    Linear,
    Conv2d,
    Conv1d,
    LSTM,
    GRU,
    Attention,
    Dropout,
    BatchNorm,
    LayerNorm,
}

/// Activation functions
#[derive(Clone, Debug)]
pub enum Activation {
    ReLU,
    LeakyReLU(f32),
    GELU,
    Swish,
    Mish,
    Softmax,
    Sigmoid,
    Tanh,
}

/// Normalization layers
#[derive(Clone, Debug)]
pub enum Normalization {
    BatchNorm,
    LayerNorm,
    InstanceNorm,
    GraphNorm,
}

/// Trait for model building
#[async_trait::async_trait]
pub trait ModelBuilder: Send + Sync {
    /// Build model architecture
    async fn build(&self, config: &ModelConfig) -> Result<Box<dyn Model>>;
}

/// Trait for deep learning models
pub trait Model: Send + Sync {
    /// Initialize model
    fn init(&mut self) -> Result<()>;

    /// Forward pass
    fn forward(&self, input: &Tensor) -> Result<Tensor>;

    /// Training step
    fn training_step(&mut self, batch: &Tensor) -> Result<f32>;

    /// Validation step
    fn validation_step(&self, batch: &Tensor) -> Result<f32>;

    /// Save model weights
    fn save_weights(&self, path: &str) -> Result<()>;

    /// Load model weights
    fn load_weights(&mut self, path: &str) -> Result<()>;

    /// Get model parameters
    fn parameters(&self) -> Vec<Tensor>;

    /// Get model configuration
    fn config(&self) -> &ModelConfig;
}

/// Shared state for ML models
#[derive(Debug)]
pub struct ModelState {
    pub step: usize,
    pub epoch: usize,
    pub train_metrics: ModelMetrics,
    pub val_metrics: ModelMetrics,
    pub best_metrics: ModelMetrics,
}

/// Model metrics
#[derive(Clone, Debug, Default)]
pub struct ModelMetrics {
    pub loss: f32,
    pub accuracy: f32,
    pub precision: f32,
    pub recall: f32,
    pub f1_score: f32,
    pub custom_metrics: std::collections::HashMap<String, f32>,
}

impl From<MLError> for AIError {
    fn from(err: MLError) -> Self {
        AIError::ModelInitError(err.to_string())
    }
}
