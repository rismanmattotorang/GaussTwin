//! Data exchange and management
//!
//! Provides robust data handling capabilities:
//! - Type-safe data exchange
//! - Serialization/deserialization
//! - Data validation
//! - Memory management

use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::{CosimError, Result};

/// Data value types supported in co-simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataValue {
    /// Real number (64-bit float)
    Real(f64),

    /// Integer (64-bit signed)
    Integer(i64),

    /// Boolean
    Boolean(bool),

    /// String
    String(String),

    /// Enumeration
    Enum {
        value: String,
        possible_values: Vec<String>,
    },

    /// Binary data
    Binary(Bytes),

    /// Array of values
    Array(Vec<DataValue>),

    /// Structured data
    Struct(HashMap<String, DataValue>),
}

impl DataValue {
    /// Get as real number
    pub fn as_real(&self) -> Result<f64> {
        match self {
            DataValue::Real(v) => Ok(*v),
            DataValue::Integer(v) => Ok(*v as f64),
            _ => Err(CosimError::DataExchange(format!(
                "Cannot convert {:?} to real",
                self
            ))),
        }
    }

    /// Get as integer
    pub fn as_integer(&self) -> Result<i64> {
        match self {
            DataValue::Integer(v) => Ok(*v),
            DataValue::Real(v) => Ok(*v as i64),
            _ => Err(CosimError::DataExchange(format!(
                "Cannot convert {:?} to integer",
                self
            ))),
        }
    }

    /// Get as boolean
    pub fn as_boolean(&self) -> Result<bool> {
        match self {
            DataValue::Boolean(v) => Ok(*v),
            _ => Err(CosimError::DataExchange(format!(
                "Cannot convert {:?} to boolean",
                self
            ))),
        }
    }

    /// Get as string
    pub fn as_string(&self) -> Result<String> {
        match self {
            DataValue::String(v) => Ok(v.clone()),
            DataValue::Enum { value, .. } => Ok(value.clone()),
            _ => Err(CosimError::DataExchange(format!(
                "Cannot convert {:?} to string",
                self
            ))),
        }
    }

    /// Get as binary
    pub fn as_binary(&self) -> Result<Bytes> {
        match self {
            DataValue::Binary(v) => Ok(v.clone()),
            _ => Err(CosimError::DataExchange(format!(
                "Cannot convert {:?} to binary",
                self
            ))),
        }
    }

    /// Validate value against schema
    pub fn validate(&self, schema: &DataSchema) -> Result<()> {
        match (self, schema) {
            (DataValue::Real(v), DataSchema::Real { min, max }) => {
                if let Some(min) = min {
                    if *v < *min {
                        return Err(CosimError::DataExchange(format!(
                            "Value {} below minimum {}",
                            v, min
                        )));
                    }
                }
                if let Some(max) = max {
                    if *v > *max {
                        return Err(CosimError::DataExchange(format!(
                            "Value {} above maximum {}",
                            v, max
                        )));
                    }
                }
            }
            (DataValue::Integer(v), DataSchema::Integer { min, max }) => {
                if let Some(min) = min {
                    if *v < *min {
                        return Err(CosimError::DataExchange(format!(
                            "Value {} below minimum {}",
                            v, min
                        )));
                    }
                }
                if let Some(max) = max {
                    if *v > *max {
                        return Err(CosimError::DataExchange(format!(
                            "Value {} above maximum {}",
                            v, max
                        )));
                    }
                }
            }
            (DataValue::String(v), DataSchema::String { pattern }) => {
                if let Some(pattern) = pattern {
                    if !pattern.is_match(v) {
                        return Err(CosimError::DataExchange(format!(
                            "Value {} does not match pattern {}",
                            v, pattern
                        )));
                    }
                }
            }
            (DataValue::Enum { value, .. }, DataSchema::Enum { values }) => {
                if !values.contains(value) {
                    return Err(CosimError::DataExchange(format!(
                        "Invalid enum value: {}",
                        value
                    )));
                }
            }
            (DataValue::Array(arr), DataSchema::Array { item_schema }) => {
                for item in arr {
                    item.validate(item_schema)?;
                }
            }
            (DataValue::Struct(map), DataSchema::Struct { fields }) => {
                for (key, schema) in fields {
                    if let Some(value) = map.get(key) {
                        value.validate(schema)?;
                    } else if schema.is_required() {
                        return Err(CosimError::DataExchange(format!(
                            "Missing required field: {}",
                            key
                        )));
                    }
                }
            }
            _ => {
                return Err(CosimError::DataExchange(format!(
                    "Type mismatch: value {:?} does not match schema {:?}",
                    self, schema
                )))
            }
        }
        Ok(())
    }
}

/// Data schema for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSchema {
    /// Real number schema
    Real { min: Option<f64>, max: Option<f64> },

    /// Integer schema
    Integer { min: Option<i64>, max: Option<i64> },

    /// Boolean schema
    Boolean,

    /// String schema
    String {
        #[serde(with = "serde_regex")]
        pattern: Option<regex::Regex>,
    },

    /// Enum schema
    Enum { values: Vec<String> },

    /// Binary schema
    Binary { max_size: Option<usize> },

    /// Array schema
    Array { item_schema: Box<DataSchema> },

    /// Struct schema
    Struct { fields: HashMap<String, DataSchema> },
}

impl DataSchema {
    /// Check if field is required
    pub fn is_required(&self) -> bool {
        // By default, all fields are required
        true
    }
}

/// Data buffer for efficient exchange
#[derive(Debug)]
pub struct DataBuffer {
    /// Buffer capacity
    capacity: usize,

    /// Current size
    size: usize,

    /// Data storage
    storage: Vec<u8>,
}

impl DataBuffer {
    /// Create new data buffer
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            size: 0,
            storage: Vec::with_capacity(capacity),
        }
    }

    /// Write data to buffer
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        if self.size + data.len() > self.capacity {
            return Err(CosimError::DataExchange(
                "Buffer capacity exceeded".to_string(),
            ));
        }
        self.storage.extend_from_slice(data);
        self.size += data.len();
        Ok(())
    }

    /// Read data from buffer
    pub fn read(&mut self, len: usize) -> Result<Vec<u8>> {
        if len > self.size {
            return Err(CosimError::DataExchange(
                "Not enough data in buffer".to_string(),
            ));
        }
        let data = self.storage[..len].to_vec();
        self.storage.drain(..len);
        self.size -= len;
        Ok(data)
    }

    /// Clear buffer
    pub fn clear(&mut self) {
        self.storage.clear();
        self.size = 0;
    }

    /// Get current size
    pub fn size(&self) -> usize {
        self.size
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Check if buffer is full
    pub fn is_full(&self) -> bool {
        self.size == self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_value_conversion() {
        let real = DataValue::Real(42.0);
        assert_eq!(real.as_real().unwrap(), 42.0);
        assert_eq!(real.as_integer().unwrap(), 42);

        let int = DataValue::Integer(42);
        assert_eq!(int.as_real().unwrap(), 42.0);
        assert_eq!(int.as_integer().unwrap(), 42);

        let string = DataValue::String("test".to_string());
        assert_eq!(string.as_string().unwrap(), "test");
    }

    #[test]
    fn test_data_validation() {
        let schema = DataSchema::Real {
            min: Some(0.0),
            max: Some(100.0),
        };

        let valid = DataValue::Real(42.0);
        assert!(valid.validate(&schema).is_ok());

        let invalid = DataValue::Real(-1.0);
        assert!(invalid.validate(&schema).is_err());
    }

    #[test]
    fn test_data_buffer() {
        let mut buffer = DataBuffer::new(100);

        // Write data
        buffer.write(&[1, 2, 3]).unwrap();
        assert_eq!(buffer.size(), 3);

        // Read data
        let data = buffer.read(2).unwrap();
        assert_eq!(data, vec![1, 2]);
        assert_eq!(buffer.size(), 1);

        // Clear buffer
        buffer.clear();
        assert!(buffer.is_empty());
    }
}
