use tch::{Device, Kind, Tensor};

/// Represents a neural network layer
pub trait Layer: Send {
    fn forward(&self, input: &Tensor) -> Tensor;
    fn parameters(&self) -> Vec<Tensor>;
}

/// Linear layer implementation
pub struct LinearLayer {
    weight: Tensor,
    bias: Tensor,
}

impl LinearLayer {
    pub fn new(input_size: i64, output_size: i64) -> Self {
        let weight = Tensor::randn(&[output_size, input_size], (Kind::Float, Device::Cpu));
        let bias = Tensor::zeros(&[output_size], (Kind::Float, Device::Cpu));
        Self { weight, bias }
    }
}

impl Layer for LinearLayer {
    fn forward(&self, input: &Tensor) -> Tensor {
        input.matmul(&self.weight.transpose(0, 1)) + &self.bias
    }

    fn parameters(&self) -> Vec<Tensor> {
        vec![self.weight.shallow_clone(), self.bias.shallow_clone()]
    }
}

/// ReLU activation layer
pub struct ReLULayer;

impl Layer for ReLULayer {
    fn forward(&self, input: &Tensor) -> Tensor {
        input.relu()
    }

    fn parameters(&self) -> Vec<Tensor> {
        vec![]
    }
}
