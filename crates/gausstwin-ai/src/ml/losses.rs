use tch::Kind;
use tch::Tensor;

/// Represents a loss function for training
pub trait Loss: Send + Sync {
    fn compute(&self, predictions: &Tensor, targets: &Tensor) -> Tensor;
}

/// Mean Squared Error loss
pub struct MSELoss;

impl Loss for MSELoss {
    fn compute(&self, predictions: &Tensor, targets: &Tensor) -> Tensor {
        (predictions - targets)
            .pow_tensor_scalar(2)
            .mean(Kind::Float)
    }
}

/// Cross Entropy loss
pub struct CrossEntropyLoss;

impl Loss for CrossEntropyLoss {
    fn compute(&self, predictions: &Tensor, targets: &Tensor) -> Tensor {
        // TODO: Implement proper cross entropy
        (predictions - targets)
            .pow_tensor_scalar(2)
            .mean(Kind::Float)
    }
}

/// Binary Cross Entropy loss
pub struct BinaryCrossEntropyLoss;

impl Loss for BinaryCrossEntropyLoss {
    fn compute(&self, predictions: &Tensor, targets: &Tensor) -> Tensor {
        // TODO: Implement proper binary cross entropy
        (predictions - targets)
            .pow_tensor_scalar(2)
            .mean(Kind::Float)
    }
}
