use std::collections::HashMap;
use tch::Tensor;

/// Represents a dataset for training
pub struct Dataset {
    pub data: Tensor,
    pub labels: Tensor,
    pub batch_size: usize,
}

impl Dataset {
    pub fn new(data: Tensor, labels: Tensor, batch_size: usize) -> Self {
        Self {
            data,
            labels,
            batch_size,
        }
    }

    pub fn len(&self) -> usize {
        self.data.size()[0] as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Represents a data loader for batching
pub struct DataLoader {
    dataset: Dataset,
    current_index: usize,
}

impl DataLoader {
    pub fn new(dataset: Dataset) -> Self {
        Self {
            dataset,
            current_index: 0,
        }
    }

    pub fn next_batch(&mut self) -> Option<(Tensor, Tensor)> {
        if self.current_index >= self.dataset.len() {
            return None;
        }

        let end_index = (self.current_index + self.dataset.batch_size).min(self.dataset.len());
        let batch_data = self.dataset.data.narrow(
            0,
            self.current_index as i64,
            (end_index - self.current_index) as i64,
        );
        let batch_labels = self.dataset.labels.narrow(
            0,
            self.current_index as i64,
            (end_index - self.current_index) as i64,
        );

        self.current_index = end_index;
        Some((batch_data, batch_labels))
    }
}
