use tch::{Device, Kind, Tensor};

/// Utility functions for ML operations
pub struct MLUtils;

impl MLUtils {
    /// Normalize tensor to zero mean and unit variance
    pub fn normalize(tensor: &Tensor) -> Tensor {
        let mean = tensor.mean(tch::Kind::Float);
        let std = tensor.std(false);
        (tensor - mean) / (std + 1e-8)
    }

    /// One-hot encode labels
    pub fn one_hot(labels: &Tensor, num_classes: i64) -> Tensor {
        let batch_size = labels.size()[0];
        let mut one_hot = Tensor::zeros(&[batch_size, num_classes], (Kind::Float, Device::Cpu));

        // Use scatter for one-hot encoding
        let indices = labels.unsqueeze(1);
        let ones = Tensor::ones(&[batch_size, num_classes], (Kind::Float, Device::Cpu));
        one_hot = one_hot.scatter(1, &indices, &ones);

        one_hot
    }

    /// Split tensor into train and validation sets
    pub fn train_val_split(tensor: &Tensor, val_ratio: f64) -> (Tensor, Tensor) {
        let total_size = tensor.size()[0];
        let val_size = (total_size as f64 * val_ratio) as i64;
        let train_size = total_size - val_size;

        let train_data = tensor.narrow(0, 0, train_size);
        let val_data = tensor.narrow(0, train_size, val_size);

        (train_data, val_data)
    }
}
