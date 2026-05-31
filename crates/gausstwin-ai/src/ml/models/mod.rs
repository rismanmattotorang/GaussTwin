pub mod gnn;
mod transformer;
mod vision;

pub use gnn::GNNModel;
pub use transformer::TransformerModel;
pub use vision::VisionModel;

use crate::ml::{Model, ModelArchitecture, ModelConfig, Result};

/// Factory for creating ML models
pub struct ModelFactory;

impl ModelFactory {
    /// Create a new model based on configuration
    pub fn create(config: ModelConfig) -> Result<Box<dyn Model>> {
        match &config.architecture {
            ModelArchitecture::GNN { .. } => Ok(Box::new(GNNModel::new(config)?)),
            ModelArchitecture::Transformer { .. } => Ok(Box::new(TransformerModel::new(config)?)),
            ModelArchitecture::Vision { .. } => Ok(Box::new(VisionModel::new(config)?)),
            ModelArchitecture::Temporal { .. } => Err(crate::ml::MLError::ArchitectureError(
                "Temporal models not yet implemented".into(),
            )
            .into()),
        }
    }
}

impl Default for ModelFactory {
    fn default() -> Self {
        ModelFactory
    }
}
