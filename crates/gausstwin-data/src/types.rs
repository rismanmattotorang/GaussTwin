use chrono::{DateTime, Utc};
use gausstwin_vec;
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// Vector data representation used by the higher-level data APIs.
/// Only the raw vector values and basic metadata are stored here so that
/// the `gausstwin-data` crate does not need to understand the full
/// `gausstwin_vec::Vector` structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorData {
    /// Raw vector values
    pub vector: Vec<f32>,

    /// Arbitrary user-supplied metadata
    pub metadata: JsonValue,

    /// Convenience field for quickly determining dimensionality
    pub dimension: usize,

    /// Optional namespace/collection name
    pub namespace: String,
}

/// Scalar (non-vector) data representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalarData {
    /// Primary value payload (can be any valid JSON)
    pub value: JsonValue,

    /// Additional metadata associated with the value
    pub metadata: JsonValue,
}

/// Hybrid data combines an optional vector with a scalar value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridData {
    pub vector: Option<Vec<f32>>,
    pub value: JsonValue,
    pub metadata: JsonValue,
}

/// Record wrapper used by batch APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridRecord {
    pub key: String,
    pub data: HybridData,
}

/// Search result returned by high-level search APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub key: String,
    pub score: f32,
    pub data: HybridData,
}

impl From<gausstwin_vec::SearchResult> for SearchResult {
    fn from(result: gausstwin_vec::SearchResult) -> Self {
        Self {
            key: result.id.clone(),
            score: result.score,
            data: HybridData {
                vector: Some(result.vector.clone()),
                value: result.metadata.clone().unwrap_or_default(),
                metadata: serde_json::json!({}),
            },
        }
    }
}

/// Query filters for scalar data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilters {
    pub metadata_filters: Option<JsonValue>,
    pub value_filters: Option<JsonValue>,
}

/// Supported value types
#[derive(Debug, Clone)]
pub enum Value {
    Vector(VectorData),
    Scalar(ScalarData),
    Hybrid(HybridData),
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Value::Vector(v) => v.serialize(serializer),
            Value::Scalar(s) => s.serialize(serializer),
            Value::Hybrid(h) => h.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let json = serde_json::Value::deserialize(deserializer)?;
        match &json {
            serde_json::Value::Object(o) => {
                if o.contains_key("vector") {
                    Ok(Value::Vector(
                        VectorData::deserialize(json.clone()).map_err(serde::de::Error::custom)?,
                    ))
                } else if o.contains_key("value") {
                    Ok(Value::Scalar(
                        ScalarData::deserialize(json.clone()).map_err(serde::de::Error::custom)?,
                    ))
                } else if o.contains_key("vector") && o.contains_key("value") {
                    Ok(Value::Hybrid(
                        HybridData::deserialize(json.clone()).map_err(serde::de::Error::custom)?,
                    ))
                } else {
                    Err(serde::de::Error::custom("Invalid format"))
                }
            }
            _ => Err(serde::de::Error::custom("Invalid format")),
        }
    }
}

/// Vector store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStoreConfig {
    pub dimension: usize,
    pub distance_type: String,
    pub index_type: String,

    // Advanced search/index parameters required by higher level layers
    pub nprobe: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbConfig {
    pub url: String,
    pub username: String,
    pub password: String,

    // Connection pooling / tuning options expected by the validation logic
    pub min_connections: usize,
    pub max_connections: usize,
    pub connect_timeout: std::time::Duration,
    pub idle_timeout: std::time::Duration,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub max_size: usize,
    /// Time-to-live for cached entries
    pub ttl: std::time::Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,
            ttl: std::time::Duration::from_secs(3600),
        }
    }
}

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub min_size: usize,
    pub max_size: usize,
    pub timeout_seconds: u64,

    // Additional optional tuning settings
    pub min_idle: usize,
    pub max_lifetime: Option<std::time::Duration>,
    pub idle_timeout: Option<std::time::Duration>,
    pub connection_timeout: std::time::Duration,
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub prefix: String,

    /// How often metrics should be exported / reported
    pub report_interval: std::time::Duration,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prefix: "gausstwin".to_string(),
            report_interval: std::time::Duration::from_secs(3600),
        }
    }
}

/// Unified store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedStoreConfig {
    pub vector_store: VectorStoreConfig,
    pub database: DbConfig,
    pub cache: Option<CacheConfig>,
    pub pool: PoolConfig,
    pub metrics: Option<MetricsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRecord {
    pub id: String,
    pub data: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub limit: usize,
    pub offset: usize,
    pub filters: Option<serde_json::Value>,
}
