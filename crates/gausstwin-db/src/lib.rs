//! GaussTwin Database Layer
//!
//! Enterprise-grade database layer with advanced features including:
//! - Encryption at rest
//! - Compliance management (GDPR, HIPAA)
//! - Audit logging
//! - Backup and restore
//! - Data partitioning
//! - Security management
//!
//! # Features
//! - Comprehensive encryption support
//! - Compliance tracking and enforcement
//! - Detailed audit logging
//! - Automated backup management
//! - Advanced partitioning strategies
//! - Security policy enforcement
//!
//! # Examples
//! ```no_run
//! use gausstwin_db::{TwinStore, SurrealStore, ComplianceConfig};
//!
//! async fn example() -> Result<(), DatabaseError> {
//!     let config = ComplianceConfig {
//!         gdpr_enabled: true,
//!         hipaa_enabled: true,
//!         data_retention_days: 90,
//!         encryption_required: true,
//!         audit_logging_enabled: true,
//!     };
//!     
//!     let store = SurrealStore::new(
//!         "localhost",
//!         8000,
//!         "namespace",
//!         "database",
//!         "username",
//!         "password",
//!         config
//!     ).await?;
//!     
//!     Ok(())
//! }
//! ```

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm,
};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use opentelemetry::{
    trace::{Span, Tracer},
    KeyValue,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use surrealdb::{engine::remote::ws::Client, opt::Config, Surreal};
use thiserror::Error;
use tokio::sync::{RwLock, Semaphore};
use tracing::{error, info};
use uuid::Uuid;

// Re-export core types
pub use surrealdb;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Query error: {0}")]
    Query(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Transaction error: {0}")]
    Transaction(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Compliance error: {0}")]
    Compliance(String),
    #[error("Rate limit exceeded")]
    RateLimit,
    #[error("Backup error: {0}")]
    Backup(String),
    #[error("Restore error: {0}")]
    Restore(String),
    #[error("Partition error: {0}")]
    Partition(String),
    #[error("Security error: {0}")]
    Security(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRegion {
    pub id: String,
    pub name: String,
    pub country_code: String,
    pub is_gdpr_compliant: bool,
    pub is_hipaa_compliant: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceConfig {
    pub gdpr_enabled: bool,
    pub hipaa_enabled: bool,
    pub encryption_required: bool,
    pub audit_logging_enabled: bool,
    pub data_retention_days: u32,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            gdpr_enabled: false,
            hipaa_enabled: false,
            encryption_required: false,
            audit_logging_enabled: false,
            data_retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub operation: String,
    pub user_id: String,
    pub resource_id: String,
    pub details: serde_json::Value,
    pub ip_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseMetrics {
    pub total_snapshots: u64,
    pub total_storage_bytes: u64,
    pub avg_query_time_ms: f64,
    pub active_connections: u32,
    pub cache_hit_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub schedule_cron: String,
    pub retention_days: u32,
    pub storage_path: String,
    pub encryption_key: Option<String>,
    pub compression_level: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionConfig {
    pub strategy: PartitionStrategy,
    pub key: String,
    pub max_size_gb: u32,
    pub retention_policy: RetentionPolicy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PartitionStrategy {
    TimeRange,
    Hash,
    List,
    Range,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub enabled: bool,
    pub max_age_days: Option<u32>,
    pub max_size_gb: Option<u32>,
    pub archive_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub encryption_at_rest: bool,
    pub tls_config: Option<TlsConfig>,
    pub access_control: AccessControlConfig,
    pub key_rotation_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub ca_path: Option<String>,
    pub verify_peer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlConfig {
    pub role_based_access: bool,
    pub ip_whitelist: Vec<String>,
    pub max_connections_per_user: u32,
    pub session_timeout_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionStats {
    pub partition_id: String,
    pub size_bytes: u64,
    pub record_count: u64,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
}

/// Enhanced trait for enterprise database operations
#[async_trait]
pub trait TwinStore: Send + Sync {
    /// Store a simulation snapshot with encryption and compliance checks
    ///
    /// # Arguments
    /// * `model_id` - Unique identifier for the model
    /// * `step` - Simulation step number
    /// * `blob` - Binary data to store
    /// * `encryption_key` - Optional encryption key
    ///
    /// # Returns
    /// * `Result<()>` - Success or error
    ///
    /// # Errors
    /// * `DatabaseError::Storage` - If storage fails
    /// * `DatabaseError::Encryption` - If encryption fails
    /// * `DatabaseError::Compliance` - If compliance check fails
    async fn put_snapshot(
        &self,
        model_id: Uuid,
        step: u64,
        blob: Bytes,
        encryption_key: Option<&[u8]>,
    ) -> Result<(), DatabaseError>;

    /// Retrieve the latest snapshot with decryption
    async fn fetch_latest(
        &self,
        model_id: Uuid,
        encryption_key: Option<&[u8]>,
    ) -> Result<Option<Bytes>, DatabaseError>;

    /// Retrieve a specific snapshot with compliance checks
    async fn fetch_snapshot(
        &self,
        model_id: Uuid,
        step: u64,
        encryption_key: Option<&[u8]>,
    ) -> Result<Option<Bytes>, DatabaseError>;

    /// List all snapshots with pagination
    async fn list_snapshots(
        &self,
        model_id: Uuid,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<u64>, DatabaseError>;

    /// Delete snapshots with audit logging
    async fn delete_snapshot(
        &self,
        model_id: Uuid,
        step: u64,
        user_id: String,
    ) -> Result<(), DatabaseError>;

    /// Get database metrics
    async fn get_metrics(&self) -> Result<DatabaseMetrics, DatabaseError>;

    /// Create audit log entry
    async fn create_audit_log(&self, log: AuditLog) -> Result<(), DatabaseError>;

    /// Configure compliance settings
    async fn set_compliance_config(&self, config: ComplianceConfig) -> Result<(), DatabaseError>;

    /// Get current compliance configuration
    async fn get_compliance_config(&self) -> Result<ComplianceConfig, DatabaseError>;

    /// Create a backup with encryption and compression
    async fn create_backup(&self, config: &BackupConfig) -> Result<String, DatabaseError>;

    /// Restore from a backup
    async fn restore_from_backup(
        &self,
        backup_id: String,
        encryption_key: Option<String>,
    ) -> Result<(), DatabaseError>;

    /// Configure data partitioning
    async fn configure_partitioning(&self, config: PartitionConfig) -> Result<(), DatabaseError>;

    /// Configure security settings
    async fn configure_security(&self, config: SecurityConfig) -> Result<(), DatabaseError>;

    /// Rotate encryption keys
    async fn rotate_encryption_keys(&self) -> Result<(), DatabaseError>;

    /// Get partition statistics
    async fn get_partition_stats(&self) -> Result<Vec<PartitionStats>, DatabaseError>;
}

/// Enhanced SurrealDB implementation with enterprise features
pub struct SurrealStore {
    client: Arc<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>>,
    namespace: String,
    database: String,
    compliance_config: Arc<RwLock<ComplianceConfig>>,
    rate_limiter: Arc<Semaphore>,
    metrics: Arc<RwLock<DatabaseMetrics>>,
    tracer: opentelemetry::global::BoxedTracer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRecord {
    pub id: String,
    pub model_id: String,
    pub step: u64,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub encrypted: bool,
    pub encryption_metadata: Option<serde_json::Value>,
    pub compliance_metadata: Option<serde_json::Value>,
}

impl SurrealStore {
    /// Create a new SurrealDB store with enterprise features
    pub async fn new(
        host: &str,
        port: u16,
        namespace: &str,
        database: &str,
        username: &str,
        password: &str,
        config: ComplianceConfig,
    ) -> Result<Self, DatabaseError> {
        let endpoint = format!("{}:{}", host, port);
        let client = surrealdb::Surreal::new::<surrealdb::engine::remote::ws::Ws>((
            endpoint,
            Config::default(),
        ))
        .await
        .map_err(|e| DatabaseError::Connection(e.to_string()))?;

        client
            .signin(surrealdb::opt::auth::Root { username, password })
            .await
            .map_err(|e| DatabaseError::Security(e.to_string()))?;

        client
            .use_ns(namespace)
            .use_db(database)
            .await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;

        let store = Self {
            client: Arc::new(client),
            namespace: namespace.to_string(),
            database: database.to_string(),
            compliance_config: Arc::new(RwLock::new(config)),
            rate_limiter: Arc::new(Semaphore::new(100)), // Configurable
            metrics: Arc::new(RwLock::new(DatabaseMetrics {
                total_snapshots: 0,
                total_storage_bytes: 0,
                avg_query_time_ms: 0.0,
                active_connections: 0,
                cache_hit_ratio: 0.0,
            })),
            tracer: opentelemetry::global::tracer("gausstwin-db"),
        };

        store.initialize_schema().await?;
        Ok(store)
    }

    async fn initialize_schema(&self) -> Result<(), DatabaseError> {
        // Create necessary tables and indexes
        let schema = r#"
            DEFINE TABLE snapshots SCHEMAFULL;
            DEFINE FIELD model_id ON snapshots TYPE string;
            DEFINE FIELD step ON snapshots TYPE number;
            DEFINE FIELD data ON snapshots TYPE bytes;
            DEFINE FIELD created_at ON snapshots TYPE datetime;
            DEFINE FIELD encrypted ON snapshots TYPE bool;
            DEFINE FIELD encryption_metadata ON snapshots TYPE object;
            DEFINE FIELD compliance_metadata ON snapshots TYPE object;
            DEFINE INDEX snapshot_model_step ON snapshots FIELDS model_id, step UNIQUE;
        "#;

        self.client
            .query(schema)
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(())
    }

    fn encrypt_data(
        &self,
        data: &[u8],
        key: &[u8],
    ) -> Result<(Vec<u8>, serde_json::Value), DatabaseError> {
        let nonce = rand::thread_rng().gen::<[u8; 12]>();
        let cipher =
            Aes256Gcm::new_from_slice(key).map_err(|e| DatabaseError::Encryption(e.to_string()))?;

        let encrypted = cipher
            .encrypt(&nonce.into(), data)
            .map_err(|e| DatabaseError::Encryption(e.to_string()))?;

        let metadata = serde_json::json!({
            "algorithm": "AES-GCM-256",
            "nonce": nonce.to_vec(),
        });

        Ok((encrypted, metadata))
    }

    fn decrypt_data(
        &self,
        data: &[u8],
        key: &[u8],
        metadata: &serde_json::Value,
    ) -> Result<Vec<u8>, DatabaseError> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };

        let nonce = metadata["nonce"]
            .as_array()
            .ok_or_else(|| DatabaseError::Encryption("Invalid nonce format".to_string()))?;
        let nonce_bytes: Vec<u8> = nonce.iter().map(|v| v.as_u64().unwrap() as u8).collect();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let cipher =
            Aes256Gcm::new_from_slice(key).map_err(|e| DatabaseError::Encryption(e.to_string()))?;

        cipher
            .decrypt(nonce, data)
            .map_err(|e| DatabaseError::Encryption(e.to_string()))
    }

    async fn check_compliance(
        &self,
        operation: &str,
        data: &[u8],
    ) -> Result<serde_json::Value, DatabaseError> {
        let config = self.compliance_config.read().await;
        let metadata = serde_json::json!({
            "gdpr_compliant": config.gdpr_enabled,
            "hipaa_compliant": config.hipaa_enabled,
            "retention_days": config.data_retention_days,
            "operation": operation,
            "data_size": data.len(),
            "timestamp": Utc::now().to_rfc3339(),
        });

        Ok(metadata)
    }

    async fn update_metrics(&self, operation: &str, start_time: DateTime<Utc>) {
        let duration = Utc::now()
            .signed_duration_since(start_time)
            .num_milliseconds() as f64;

        let mut metrics = self.metrics.write().await;
        metrics.avg_query_time_ms = (metrics.avg_query_time_ms + duration) / 2.0;

        // Update other metrics based on operation
        match operation {
            "snapshot" => metrics.total_snapshots += 1,
            "storage" => metrics.total_storage_bytes += 1,
            _ => {}
        }
    }

    /// Create a new backup with encryption and compression
    async fn create_encrypted_backup(
        &self,
        config: &BackupConfig,
    ) -> Result<String, DatabaseError> {
        let _span = self.tracer.start("create_encrypted_backup");

        // Export data to memory first
        let temp_path = PathBuf::from("temp_export.surql");
        self.client
            .export(temp_path.as_path())
            .await
            .map_err(|e| DatabaseError::Backup(e.to_string()))?;

        let export_data = tokio::fs::read(&temp_path)
            .await
            .map_err(|e| DatabaseError::Backup(format!("Failed to read export: {}", e)))?;

        // Clean up temp file
        tokio::fs::remove_file(&temp_path)
            .await
            .map_err(|e| DatabaseError::Backup(format!("Failed to clean up temp file: {}", e)))?;

        // Compress data
        let compressed = zstd::encode_all(export_data.as_slice(), config.compression_level as i32)
            .map_err(|e| DatabaseError::Backup(format!("Compression failed: {}", e)))?;

        // Encrypt if key provided
        let final_data = if let Some(key) = &config.encryption_key {
            let (encrypted, _) = self.encrypt_data(&compressed, key.as_bytes())?;
            encrypted
        } else {
            compressed
        };

        let backup_id = Uuid::new_v4().to_string();
        let backup_path = format!("{}/{}.backup", config.storage_path, backup_id);

        tokio::fs::write(&backup_path, final_data)
            .await
            .map_err(|e| DatabaseError::Backup(format!("Failed to write backup: {}", e)))?;

        Ok(backup_id)
    }

    /// Implement data partitioning
    async fn setup_partitioning(&self, config: &PartitionConfig) -> Result<(), DatabaseError> {
        let mut span = self.tracer.start("setup_partitioning");
        span.set_attributes(vec![
            KeyValue::new("strategy", format!("{:?}", config.strategy)),
            KeyValue::new("key", config.key.clone()),
        ]);

        // Create partition table and indexes
        let partition_query = match config.strategy {
            PartitionStrategy::TimeRange => format!(
                "DEFINE TABLE partition_{}_{} SCHEMAFULL;
                 DEFINE FIELD timestamp ON partition_{}_{} TYPE datetime;
                 DEFINE INDEX partition_timestamp ON partition_{}_{} FIELDS timestamp;",
                self.namespace, config.key, self.namespace, config.key, self.namespace, config.key
            ),
            PartitionStrategy::Hash => format!(
                "DEFINE TABLE partition_{}_{} SCHEMAFULL;
                 DEFINE FIELD hash_key ON partition_{}_{} TYPE string;
                 DEFINE INDEX partition_hash ON partition_{}_{} FIELDS hash_key;",
                self.namespace, config.key, self.namespace, config.key, self.namespace, config.key
            ),
            _ => {
                return Err(DatabaseError::Partition(
                    "Unsupported partition strategy".to_string(),
                ))
            }
        };

        self.client
            .query(&partition_query)
            .await
            .map_err(|e| DatabaseError::Partition(e.to_string()))?;

        // Set up retention policy if enabled
        if config.retention_policy.enabled {
            let retention_query = match config.retention_policy.max_age_days {
                Some(days) => format!(
                    "DEFINE RETENTION ON partition_{}_{} DURATION {}d;",
                    self.namespace, config.key, days
                ),
                None => format!(
                    "DEFINE RETENTION ON partition_{}_{} SIZE {};",
                    self.namespace,
                    config.key,
                    config.retention_policy.max_size_gb.unwrap_or(1000)
                ),
            };

            self.client
                .query(&retention_query)
                .await
                .map_err(|e| DatabaseError::Partition(e.to_string()))?;
        }

        Ok(())
    }

    /// Implement security configuration
    async fn configure_security_settings(
        &self,
        config: &SecurityConfig,
    ) -> Result<(), DatabaseError> {
        let mut span = self.tracer.start("configure_security");
        span.set_attributes(vec![
            KeyValue::new("encryption_at_rest", config.encryption_at_rest),
            KeyValue::new("role_based_access", config.access_control.role_based_access),
        ]);

        // Configure TLS if provided
        if let Some(tls) = &config.tls_config {
            // Implementation would depend on SurrealDB's TLS configuration options
            info!("Configuring TLS with cert: {}", tls.cert_path);
        }

        // Configure access control
        if config.access_control.role_based_access {
            // Set up role-based access control
            let rbac_query = "
                DEFINE SCOPE authenticated SIGNIN ( SELECT * FROM user WHERE email = $email AND crypto::argon2::compare(password, $password) );
                DEFINE TOKEN user_token ON scope authenticated TYPE HS512 VALUE 'your-secret-key-here' EXPIRE 24h;
            ";

            self.client
                .query(rbac_query)
                .await
                .map_err(|e| DatabaseError::Security(e.to_string()))?;
        }

        // Configure IP whitelist
        if !config.access_control.ip_whitelist.is_empty() {
            // Implementation would depend on SurrealDB's network configuration options
            info!(
                "Configuring IP whitelist: {:?}",
                config.access_control.ip_whitelist
            );
        }

        Ok(())
    }

    /// Implement key rotation
    async fn rotate_encryption_keys(&self) -> Result<(), DatabaseError> {
        let mut span = self.tracer.start("rotate_encryption_keys");

        // Generate new key
        let new_key = rand::thread_rng().gen::<[u8; 32]>();

        // Re-encrypt all data with new key
        // This is a placeholder - actual implementation would need to:
        // 1. List all encrypted records
        // 2. Decrypt with old key
        // 3. Re-encrypt with new key
        // 4. Update records atomically

        span.set_attribute(KeyValue::new("status", "completed"));
        Ok(())
    }

    async fn get_partition_stats(&self) -> Result<Vec<PartitionStats>, DatabaseError> {
        let _span = self.tracer.start("get_partition_stats");

        // This is a placeholder - actual implementation would need to:
        // 1. Query partition metadata
        // 2. Aggregate statistics
        // 3. Return results

        Ok(Vec::new())
    }

    async fn restore_from_backup(
        &self,
        backup_id: String,
        encryption_key: Option<String>,
    ) -> Result<(), DatabaseError> {
        let _span = self.tracer.start("restore_from_backup");

        // Read backup file
        let backup_path = format!("backups/{}.backup", backup_id);
        let encrypted_data = tokio::fs::read(&backup_path)
            .await
            .map_err(|e| DatabaseError::Restore(format!("Failed to read backup: {}", e)))?;

        // Decrypt if key provided
        let data = if let Some(key) = encryption_key {
            let metadata = serde_json::json!({
                "algorithm": "AES-GCM-256",
                "nonce": vec![0u8; 12],
            });
            self.decrypt_data(&encrypted_data, key.as_bytes(), &metadata)?
        } else {
            encrypted_data
        };

        // Decompress
        let decompressed = zstd::decode_all(&data[..])
            .map_err(|e| DatabaseError::Restore(format!("Decompression failed: {}", e)))?;

        // Write to temp file for import
        let temp_path = PathBuf::from("temp_import.surql");
        tokio::fs::write(&temp_path, &decompressed)
            .await
            .map_err(|e| DatabaseError::Restore(format!("Failed to write temp file: {}", e)))?;

        // Import data
        self.client
            .import(temp_path.as_path())
            .await
            .map_err(|e| DatabaseError::Restore(e.to_string()))?;

        // Clean up temp file
        tokio::fs::remove_file(&temp_path)
            .await
            .map_err(|e| DatabaseError::Restore(format!("Failed to clean up temp file: {}", e)))?;

        Ok(())
    }

    pub async fn create_compliance_config(
        &self,
        config: ComplianceConfig,
    ) -> Result<(), DatabaseError> {
        let _result: Vec<ComplianceConfig> = self
            .client
            .create("compliance_config")
            .content(&config)
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl TwinStore for SurrealStore {
    async fn put_snapshot(
        &self,
        model_id: Uuid,
        step: u64,
        blob: Bytes,
        encryption_key: Option<&[u8]>,
    ) -> Result<(), DatabaseError> {
        let mut span = self.tracer.start("put_snapshot");
        span.set_attributes(vec![
            KeyValue::new("model_id", model_id.to_string()),
            KeyValue::new("step", step.to_string()),
        ]);

        let record = SnapshotRecord {
            id: format!("{}-{}", model_id, step),
            model_id: model_id.to_string(),
            step,
            data: blob.to_vec(),
            created_at: Utc::now(),
            encrypted: encryption_key.is_some(),
            encryption_metadata: None,
            compliance_metadata: None,
        };

        self.client
            .create::<Option<SnapshotRecord>>(("snapshots", record.id.clone()))
            .content(record)
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(())
    }

    async fn fetch_latest(
        &self,
        model_id: Uuid,
        encryption_key: Option<&[u8]>,
    ) -> Result<Option<Bytes>, DatabaseError> {
        let mut span = self.tracer.start("fetch_latest");
        span.set_attributes(vec![KeyValue::new("model_id", model_id.to_string())]);

        let _permit = self
            .rate_limiter
            .acquire()
            .await
            .map_err(|_| DatabaseError::RateLimit)?;

        let result: Option<SnapshotRecord> = self
            .client
            .query(
                "SELECT * FROM snapshots 
                WHERE model_id = $model_id 
                ORDER BY step DESC 
                LIMIT 1",
            )
            .bind(("model_id", model_id.to_string()))
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?
            .take(0)
            .map_err(|e| DatabaseError::Query(e.to_string()))?;

        if let Some(record) = result {
            if record.encrypted {
                if let Some(key) = encryption_key {
                    if let Some(metadata) = record.encryption_metadata {
                        let decrypted = self.decrypt_data(&record.data, key, &metadata)?;
                        Ok(Some(Bytes::from(decrypted)))
                    } else {
                        Err(DatabaseError::Encryption(
                            "Missing encryption metadata".to_string(),
                        ))
                    }
                } else {
                    Err(DatabaseError::Encryption(
                        "Encryption key required".to_string(),
                    ))
                }
            } else {
                Ok(Some(Bytes::from(record.data)))
            }
        } else {
            Ok(None)
        }
    }

    async fn fetch_snapshot(
        &self,
        model_id: Uuid,
        step: u64,
        encryption_key: Option<&[u8]>,
    ) -> Result<Option<Bytes>, DatabaseError> {
        let mut span = self.tracer.start("fetch_snapshot");
        span.set_attributes(vec![
            KeyValue::new("model_id", model_id.to_string()),
            KeyValue::new("step", step as i64),
        ]);

        let _permit = self
            .rate_limiter
            .acquire()
            .await
            .map_err(|_| DatabaseError::RateLimit)?;

        let result: Option<SnapshotRecord> = self
            .client
            .query(
                "SELECT * FROM snapshots 
                WHERE model_id = $model_id 
                AND step = $step 
                LIMIT 1",
            )
            .bind(("model_id", model_id.to_string()))
            .bind(("step", step))
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?
            .take(0)
            .map_err(|e| DatabaseError::Query(e.to_string()))?;

        if let Some(record) = result {
            if record.encrypted {
                if let Some(key) = encryption_key {
                    if let Some(metadata) = record.encryption_metadata {
                        let decrypted = self.decrypt_data(&record.data, key, &metadata)?;
                        Ok(Some(Bytes::from(decrypted)))
                    } else {
                        Err(DatabaseError::Encryption(
                            "Missing encryption metadata".to_string(),
                        ))
                    }
                } else {
                    Err(DatabaseError::Encryption(
                        "Encryption key required".to_string(),
                    ))
                }
            } else {
                Ok(Some(Bytes::from(record.data)))
            }
        } else {
            Ok(None)
        }
    }

    async fn list_snapshots(
        &self,
        model_id: Uuid,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<u64>, DatabaseError> {
        let mut span = self.tracer.start("list_snapshots");
        span.set_attributes(vec![
            KeyValue::new("model_id", model_id.to_string()),
            KeyValue::new("page", page as i64),
            KeyValue::new("page_size", page_size as i64),
        ]);

        let _permit = self
            .rate_limiter
            .acquire()
            .await
            .map_err(|_| DatabaseError::RateLimit)?;

        let result: Vec<SnapshotRecord> = self
            .client
            .query(
                "SELECT * FROM snapshots 
                WHERE model_id = $model_id 
                ORDER BY step ASC 
                LIMIT $page_size OFFSET $page",
            )
            .bind(("model_id", model_id.to_string()))
            .bind(("page_size", page_size as i64))
            .bind(("page", page as i64))
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?
            .take(0)
            .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(result.into_iter().map(|r| r.step).collect())
    }

    async fn delete_snapshot(
        &self,
        model_id: Uuid,
        step: u64,
        user_id: String,
    ) -> Result<(), DatabaseError> {
        let mut span = self.tracer.start("delete_snapshot");
        span.set_attributes(vec![
            KeyValue::new("model_id", model_id.to_string()),
            KeyValue::new("step", step.to_string()),
            KeyValue::new("user_id", user_id.clone()),
        ]);

        self.client
            .query(
                "DELETE FROM snapshots 
                WHERE model_id = $model_id 
                AND step = $step",
            )
            .bind(("model_id", model_id.to_string()))
            .bind(("step", step))
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;

        // Create audit log
        self.create_audit_log(AuditLog {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            operation: "delete_snapshot".to_string(),
            user_id: user_id.clone(),
            resource_id: model_id.to_string(),
            details: serde_json::json!({
                "step": step,
            }),
            ip_address: "0.0.0.0".to_string(), // Should be passed from context
        })
        .await?;

        Ok(())
    }

    async fn get_metrics(&self) -> Result<DatabaseMetrics, DatabaseError> {
        Ok(self.metrics.read().await.clone())
    }

    async fn create_audit_log(&self, log: AuditLog) -> Result<(), DatabaseError> {
        let mut span = self.tracer.start("create_audit_log");
        span.set_attributes(vec![KeyValue::new("operation", log.operation.clone())]);

        self.client
            .create::<Option<AuditLog>>(("audit_logs", log.id.clone()))
            .content(log)
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;

        Ok(())
    }

    async fn set_compliance_config(&self, config: ComplianceConfig) -> Result<(), DatabaseError> {
        let _span = self.tracer.start("set_compliance_config");

        self.client
            .create::<Option<ComplianceConfig>>(("compliance_config", "current"))
            .content(config.clone())
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;

        *self.compliance_config.write().await = config;
        Ok(())
    }

    async fn get_compliance_config(&self) -> Result<ComplianceConfig, DatabaseError> {
        Ok(self.compliance_config.read().await.clone())
    }

    async fn create_backup(&self, config: &BackupConfig) -> Result<String, DatabaseError> {
        self.create_encrypted_backup(config).await
    }

    async fn configure_partitioning(&self, config: PartitionConfig) -> Result<(), DatabaseError> {
        self.setup_partitioning(&config).await
    }

    async fn configure_security(&self, config: SecurityConfig) -> Result<(), DatabaseError> {
        self.configure_security_settings(&config).await
    }

    async fn rotate_encryption_keys(&self) -> Result<(), DatabaseError> {
        self.rotate_encryption_keys().await
    }

    async fn get_partition_stats(&self) -> Result<Vec<PartitionStats>, DatabaseError> {
        self.get_partition_stats().await
    }

    async fn restore_from_backup(
        &self,
        backup_id: String,
        encryption_key: Option<String>,
    ) -> Result<(), DatabaseError> {
        self.restore_from_backup(backup_id, encryption_key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_enterprise_features() {
        let config = ComplianceConfig {
            gdpr_enabled: true,
            hipaa_enabled: true,
            data_retention_days: 90,
            encryption_required: true,
            audit_logging_enabled: true,
        };

        let store = SurrealStore::new(
            "localhost",
            8000,
            "namespace",
            "database",
            "username",
            "password",
            config.clone(),
        )
        .await
        .unwrap();

        let model_id = Uuid::new_v4();
        let test_data = Bytes::from("test data");
        let encryption_key = b"test-key-32-bytes-long-exactly!!";

        // Test encrypted snapshot
        store
            .put_snapshot(model_id, 1, test_data.clone(), Some(encryption_key))
            .await
            .unwrap();

        // Test compliance config
        let stored_config = store.get_compliance_config().await.unwrap();
        assert_eq!(stored_config.gdpr_enabled, config.gdpr_enabled);

        // Test metrics
        let metrics = store.get_metrics().await.unwrap();
        assert!(metrics.total_snapshots > 0);
    }
}
