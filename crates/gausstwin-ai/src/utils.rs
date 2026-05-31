use std::collections::HashMap;

/// General utility functions for the AI system
pub struct Utils;

impl Utils {
    /// Convert a vector to a hash map
    pub fn vec_to_map<T: Clone>(keys: &[String], values: &[T]) -> HashMap<String, T> {
        keys.iter()
            .zip(values.iter())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Convert a hash map to vectors
    pub fn map_to_vecs<T: Clone>(map: &HashMap<String, T>) -> (Vec<String>, Vec<T>) {
        let keys: Vec<String> = map.keys().cloned().collect();
        let values: Vec<T> = map.values().cloned().collect();
        (keys, values)
    }

    /// Calculate mean of a vector
    pub fn mean(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        values.iter().sum::<f64>() / values.len() as f64
    }

    /// Calculate standard deviation of a vector
    pub fn std(values: &[f64]) -> f64 {
        if values.len() < 2 {
            return 0.0;
        }
        let mean = Self::mean(values);
        let variance =
            values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
        variance.sqrt()
    }
}
