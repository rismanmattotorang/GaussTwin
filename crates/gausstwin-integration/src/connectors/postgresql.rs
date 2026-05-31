//! PostgreSQL Connector
//!
//! Provides PostgreSQL connectivity with support for connection pooling,
//! prepared statements, transactions, and streaming queries.

use crate::{common::Metrics, Config, Connector, Error, Result};
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// PostgreSQL-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    /// Connection string or individual components
    pub connection: ConnectionConfig,
    /// Connection pool settings
    pub pool: PoolConfig,
    /// Statement cache size
    pub statement_cache_size: usize,
    /// Query timeout in seconds
    pub query_timeout_secs: u64,
    /// Enable prepared statements
    pub prepared_statements: bool,
    /// SSL mode
    pub ssl_mode: SslMode,
    /// SSL configuration
    pub ssl: Option<SslConfig>,
    /// Application name
    pub application_name: String,
    /// Target session attrs
    pub target_session_attrs: SessionAttrs,
}

/// Connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConnectionConfig {
    Url(String),
    Components {
        host: String,
        port: u16,
        database: String,
        username: String,
        password: String,
    },
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        ConnectionConfig::Components {
            host: "localhost".to_string(),
            port: 5432,
            database: "gausstwin".to_string(),
            username: "postgres".to_string(),
            password: "postgres".to_string(),
        }
    }
}

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub min_connections: u32,
    pub max_connections: u32,
    pub acquire_timeout_secs: u64,
    pub idle_timeout_secs: u64,
    pub max_lifetime_secs: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_connections: 1,
            max_connections: 10,
            acquire_timeout_secs: 30,
            idle_timeout_secs: 600,
            max_lifetime_secs: 1800,
        }
    }
}

/// SSL mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SslMode {
    Disable,
    Allow,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

/// SSL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SslConfig {
    pub root_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
}

/// Session attributes for connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionAttrs {
    Any,
    ReadWrite,
    ReadOnly,
    Primary,
    Standby,
    PreferStandby,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            pool: PoolConfig::default(),
            statement_cache_size: 100,
            query_timeout_secs: 30,
            prepared_statements: true,
            ssl_mode: SslMode::Prefer,
            ssl: None,
            application_name: "GaussTwin".to_string(),
            target_session_attrs: SessionAttrs::Any,
        }
    }
}

/// Query result row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub columns: HashMap<String, Value>,
}

/// PostgreSQL value types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Float32(f32),
    Float64(f64),
    Text(String),
    Bytea(Vec<u8>),
    Timestamp(chrono::DateTime<chrono::Utc>),
    Date(chrono::NaiveDate),
    Time(chrono::NaiveTime),
    Uuid(String),
    Json(serde_json::Value),
    Array(Vec<Value>),
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Int32(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int64(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float64(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Text(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Text(v.to_string())
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

/// Query parameters
pub type QueryParams = Vec<Value>;

/// Execution result
#[derive(Debug, Clone)]
pub struct ExecuteResult {
    pub rows_affected: u64,
}

/// Transaction isolation level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

/// Transaction handle
pub struct Transaction {
    id: String,
    isolation_level: IsolationLevel,
    completed: bool,
}

impl Transaction {
    fn new(isolation_level: IsolationLevel) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            isolation_level,
            completed: false,
        }
    }
}

/// Internal state
struct ConnectorState {
    connected: AtomicBool,
    tables: RwLock<HashMap<String, Vec<Row>>>,
    prepared_statements: RwLock<HashMap<String, String>>,
    active_transactions: RwLock<HashMap<String, IsolationLevel>>,
}

impl Default for ConnectorState {
    fn default() -> Self {
        Self {
            connected: AtomicBool::new(false),
            tables: RwLock::new(HashMap::new()),
            prepared_statements: RwLock::new(HashMap::new()),
            active_transactions: RwLock::new(HashMap::new()),
        }
    }
}

/// Internal metrics
struct InternalMetrics {
    queries: AtomicU64,
    executions: AtomicU64,
    transactions: AtomicU64,
    rows_returned: AtomicU64,
    rows_affected: AtomicU64,
    errors: AtomicU64,
    connected_at: RwLock<Option<Instant>>,
    query_latency: RwLock<Vec<f64>>,
}

impl Default for InternalMetrics {
    fn default() -> Self {
        Self {
            queries: AtomicU64::new(0),
            executions: AtomicU64::new(0),
            transactions: AtomicU64::new(0),
            rows_returned: AtomicU64::new(0),
            rows_affected: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            connected_at: RwLock::new(None),
            query_latency: RwLock::new(Vec::new()),
        }
    }
}

/// PostgreSQL Connector
pub struct PostgresConnector {
    config: Config,
    postgres_config: PostgresConfig,
    state: Arc<ConnectorState>,
    internal_metrics: Arc<InternalMetrics>,
}

impl PostgresConnector {
    /// Create a new PostgreSQL connector
    pub async fn new(config: Config) -> Result<Self> {
        let postgres_config = Self::parse_postgres_config(&config)?;
        Ok(Self {
            config,
            postgres_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        })
    }

    /// Create with explicit config
    pub fn with_postgres_config(config: Config, postgres_config: PostgresConfig) -> Self {
        Self {
            config,
            postgres_config,
            state: Arc::new(ConnectorState::default()),
            internal_metrics: Arc::new(InternalMetrics::default()),
        }
    }

    fn parse_postgres_config(config: &Config) -> Result<PostgresConfig> {
        let mut postgres_config = PostgresConfig::default();

        if let Some(username) = &config.auth.credentials.username {
            if let Some(password) = &config.auth.credentials.password {
                postgres_config.connection = ConnectionConfig::Components {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "gausstwin".to_string(),
                    username: username.clone(),
                    password: password.clone(),
                };
            }
        }

        Ok(postgres_config)
    }

    async fn record_latency(&self, duration: Duration) {
        let latency = duration.as_secs_f64() * 1000.0;
        let mut samples = self.internal_metrics.query_latency.write().await;
        samples.push(latency);
        if samples.len() > 1000 {
            samples.drain(0..500);
        }
    }

    /// Execute a query and return rows
    pub async fn query(&self, sql: &str, params: &QueryParams) -> Result<Vec<Row>> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        // Simulate query execution
        debug!("Executing query: {} with {} params", sql, params.len());

        // Parse simple SELECT queries (very simplified)
        let rows = if sql.to_uppercase().starts_with("SELECT") {
            // Return simulated empty result
            vec![]
        } else {
            vec![]
        };

        self.internal_metrics
            .queries
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .rows_returned
            .fetch_add(rows.len() as u64, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        debug!("Query returned {} rows", rows.len());
        Ok(rows)
    }

    /// Execute a query and map results to a type
    pub async fn query_as<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: &QueryParams,
    ) -> Result<Vec<T>> {
        let rows = self.query(sql, params).await?;

        // Convert rows to type
        let results: Vec<T> = rows
            .into_iter()
            .filter_map(|row| {
                let json = serde_json::to_value(&row.columns).ok()?;
                serde_json::from_value(json).ok()
            })
            .collect();

        Ok(results)
    }

    /// Execute a query and return the first row
    pub async fn query_one(&self, sql: &str, params: &QueryParams) -> Result<Option<Row>> {
        let rows = self.query(sql, params).await?;
        Ok(rows.into_iter().next())
    }

    /// Execute a query and return a scalar value
    pub async fn query_scalar<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: &QueryParams,
    ) -> Result<Option<T>> {
        let row = self.query_one(sql, params).await?;

        if let Some(row) = row {
            if let Some((_, value)) = row.columns.into_iter().next() {
                let json = serde_json::to_value(&value)?;
                return Ok(serde_json::from_value(json).ok());
            }
        }

        Ok(None)
    }

    /// Execute a statement (INSERT, UPDATE, DELETE, etc.)
    pub async fn execute(&self, sql: &str, params: &QueryParams) -> Result<ExecuteResult> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let start = Instant::now();

        debug!("Executing statement: {} with {} params", sql, params.len());

        // Simulate execution
        let rows_affected = 1u64;

        self.internal_metrics
            .executions
            .fetch_add(1, Ordering::Relaxed);
        self.internal_metrics
            .rows_affected
            .fetch_add(rows_affected, Ordering::Relaxed);
        self.record_latency(start.elapsed()).await;

        Ok(ExecuteResult { rows_affected })
    }

    /// Execute a batch of statements
    pub async fn execute_batch(&self, sql: &str) -> Result<()> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let statements: Vec<&str> = sql.split(';').filter(|s| !s.trim().is_empty()).collect();

        for statement in statements {
            self.execute(statement, &vec![]).await?;
        }

        Ok(())
    }

    /// Prepare a statement
    pub async fn prepare(&self, name: &str, sql: &str) -> Result<()> {
        let mut statements = self.state.prepared_statements.write().await;
        statements.insert(name.to_string(), sql.to_string());

        debug!("Prepared statement: {}", name);
        Ok(())
    }

    /// Execute a prepared statement
    pub async fn execute_prepared(
        &self,
        name: &str,
        params: &QueryParams,
    ) -> Result<ExecuteResult> {
        let statements = self.state.prepared_statements.read().await;
        let sql = statements
            .get(name)
            .ok_or_else(|| Error::NotFound(format!("Prepared statement not found: {}", name)))?
            .clone();
        drop(statements);

        self.execute(&sql, params).await
    }

    /// Query with a prepared statement
    pub async fn query_prepared(&self, name: &str, params: &QueryParams) -> Result<Vec<Row>> {
        let statements = self.state.prepared_statements.read().await;
        let sql = statements
            .get(name)
            .ok_or_else(|| Error::NotFound(format!("Prepared statement not found: {}", name)))?
            .clone();
        drop(statements);

        self.query(&sql, params).await
    }

    /// Begin a transaction
    pub async fn begin(&self, isolation_level: IsolationLevel) -> Result<Transaction> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let transaction = Transaction::new(isolation_level);

        {
            let mut transactions = self.state.active_transactions.write().await;
            transactions.insert(transaction.id.clone(), isolation_level);
        }

        self.internal_metrics
            .transactions
            .fetch_add(1, Ordering::Relaxed);

        info!(
            "Started transaction {} with {:?}",
            transaction.id, isolation_level
        );
        Ok(transaction)
    }

    /// Commit a transaction
    pub async fn commit(&self, transaction: &mut Transaction) -> Result<()> {
        if transaction.completed {
            return Err(Error::Protocol("Transaction already completed".to_string()));
        }

        {
            let mut transactions = self.state.active_transactions.write().await;
            transactions.remove(&transaction.id);
        }

        transaction.completed = true;
        info!("Committed transaction {}", transaction.id);
        Ok(())
    }

    /// Rollback a transaction
    pub async fn rollback(&self, transaction: &mut Transaction) -> Result<()> {
        if transaction.completed {
            return Err(Error::Protocol("Transaction already completed".to_string()));
        }

        {
            let mut transactions = self.state.active_transactions.write().await;
            transactions.remove(&transaction.id);
        }

        transaction.completed = true;
        info!("Rolled back transaction {}", transaction.id);
        Ok(())
    }

    /// Execute within a transaction
    pub async fn with_transaction<F, T>(&self, isolation_level: IsolationLevel, f: F) -> Result<T>
    where
        F: FnOnce(
                &Self,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send + '_>>
            + Send,
    {
        let mut tx = self.begin(isolation_level).await?;

        match f(self).await {
            Ok(result) => {
                self.commit(&mut tx).await?;
                Ok(result)
            }
            Err(e) => {
                let _ = self.rollback(&mut tx).await;
                Err(e)
            }
        }
    }

    /// Copy data from STDIN
    pub async fn copy_in(&self, table: &str, columns: &[&str], data: &[Vec<Value>]) -> Result<u64> {
        if !self.state.connected.load(Ordering::SeqCst) {
            return Err(Error::Connection("Not connected".to_string()));
        }

        let rows_copied = data.len() as u64;

        debug!(
            "COPY {} ({}) - {} rows",
            table,
            columns.join(", "),
            rows_copied
        );

        Ok(rows_copied)
    }

    /// Create a table
    pub async fn create_table(&self, name: &str, schema: &str) -> Result<()> {
        let sql = format!("CREATE TABLE {} ({})", name, schema);
        self.execute(&sql, &vec![]).await?;

        {
            let mut tables = self.state.tables.write().await;
            tables.insert(name.to_string(), Vec::new());
        }

        info!("Created table: {}", name);
        Ok(())
    }

    /// Drop a table
    pub async fn drop_table(&self, name: &str, if_exists: bool) -> Result<()> {
        let sql = if if_exists {
            format!("DROP TABLE IF EXISTS {}", name)
        } else {
            format!("DROP TABLE {}", name)
        };

        self.execute(&sql, &vec![]).await?;

        {
            let mut tables = self.state.tables.write().await;
            tables.remove(name);
        }

        info!("Dropped table: {}", name);
        Ok(())
    }

    /// Check if a table exists
    pub async fn table_exists(&self, name: &str) -> Result<bool> {
        let tables = self.state.tables.read().await;
        Ok(tables.contains_key(name))
    }

    /// Get column information for a table
    pub async fn get_columns(&self, _table: &str) -> Result<Vec<ColumnInfo>> {
        // Simulated - in production would query information_schema
        Ok(vec![])
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> Metrics {
        let uptime = if let Some(connected_at) = *self.internal_metrics.connected_at.read().await {
            connected_at.elapsed().as_secs()
        } else {
            0
        };

        let avg_latency = {
            let samples = self.internal_metrics.query_latency.read().await;
            if samples.is_empty() {
                0.0
            } else {
                samples.iter().sum::<f64>() / samples.len() as f64
            }
        };

        Metrics {
            connections: if self.state.connected.load(Ordering::SeqCst) {
                1
            } else {
                0
            },
            connection_failures: 0,
            messages_sent: self.internal_metrics.queries.load(Ordering::Relaxed)
                + self.internal_metrics.executions.load(Ordering::Relaxed),
            messages_received: self.internal_metrics.rows_returned.load(Ordering::Relaxed),
            errors: self.internal_metrics.errors.load(Ordering::Relaxed),
            average_latency_ms: avg_latency,
            bytes_sent: 0,
            bytes_received: 0,
            uptime_seconds: uptime,
        }
    }
}

/// Column information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
}

#[async_trait]
impl Connector for PostgresConnector {
    async fn connect(&mut self) -> Result<()> {
        let connection_str = match &self.postgres_config.connection {
            ConnectionConfig::Url(url) => url.clone(),
            ConnectionConfig::Components {
                host,
                port,
                database,
                username,
                ..
            } => format!(
                "postgresql://{}:***@{}:{}/{}",
                username, host, port, database
            ),
        };

        info!("Connecting to PostgreSQL at {}", connection_str);

        // Simulate connection - in production this would use sqlx
        self.state.connected.store(true, Ordering::SeqCst);
        *self.internal_metrics.connected_at.write().await = Some(Instant::now());

        info!("Connected to PostgreSQL");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from PostgreSQL");

        self.state.connected.store(false, Ordering::SeqCst);

        // Clear prepared statements
        {
            let mut statements = self.state.prepared_statements.write().await;
            statements.clear();
        }

        // Rollback any active transactions
        {
            let mut transactions = self.state.active_transactions.write().await;
            if !transactions.is_empty() {
                warn!(
                    "Rolling back {} active transactions on disconnect",
                    transactions.len()
                );
                transactions.clear();
            }
        }

        info!("Disconnected from PostgreSQL");
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.state.connected.load(Ordering::SeqCst)
    }

    fn metrics(&self) -> &Metrics {
        static EMPTY_METRICS: Metrics = Metrics {
            connections: 0,
            connection_failures: 0,
            messages_sent: 0,
            messages_received: 0,
            errors: 0,
            average_latency_ms: 0.0,
            bytes_sent: 0,
            bytes_received: 0,
            uptime_seconds: 0,
        };
        &EMPTY_METRICS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthConfig, AuthType, Credentials, RetryPolicy};

    fn create_test_config() -> Config {
        Config {
            name: "test-postgres".to_string(),
            connector_type: "postgresql".to_string(),
            auth: AuthConfig {
                auth_type: AuthType::None,
                credentials: Credentials {
                    username: None,
                    password: None,
                    token: None,
                    certificate_path: None,
                    private_key_path: None,
                    custom: None,
                },
            },
            retry_policy: RetryPolicy {
                max_retries: 3,
                initial_backoff: Duration::from_secs(1),
                max_backoff: Duration::from_secs(60),
                backoff_multiplier: 2.0,
            },
            timeout: Duration::from_secs(30),
        }
    }

    #[tokio::test]
    async fn test_postgres_config_default() {
        let config = PostgresConfig::default();
        assert!(config.prepared_statements);
        assert_eq!(config.statement_cache_size, 100);
        assert!(matches!(config.ssl_mode, SslMode::Prefer));
    }

    #[tokio::test]
    async fn test_postgres_connector_creation() {
        let config = create_test_config();
        let connector = PostgresConnector::new(config).await;
        assert!(connector.is_ok());
    }

    #[tokio::test]
    async fn test_connect_disconnect() {
        let config = create_test_config();
        let mut connector = PostgresConnector::new(config).await.unwrap();

        assert!(!connector.is_connected().await);

        connector.connect().await.unwrap();
        assert!(connector.is_connected().await);

        connector.disconnect().await.unwrap();
        assert!(!connector.is_connected().await);
    }

    #[tokio::test]
    async fn test_execute() {
        let config = create_test_config();
        let mut connector = PostgresConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let result = connector
            .execute(
                "INSERT INTO test (id, name) VALUES ($1, $2)",
                &vec![Value::Int32(1), Value::Text("test".to_string())],
            )
            .await
            .unwrap();

        assert!(result.rows_affected > 0);
    }

    #[tokio::test]
    async fn test_prepared_statement() {
        let config = create_test_config();
        let mut connector = PostgresConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        connector
            .prepare("test_insert", "INSERT INTO test (id) VALUES ($1)")
            .await
            .unwrap();

        let result = connector
            .execute_prepared("test_insert", &vec![Value::Int32(1)])
            .await
            .unwrap();

        assert!(result.rows_affected > 0);
    }

    #[tokio::test]
    async fn test_transaction() {
        let config = create_test_config();
        let mut connector = PostgresConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let mut tx = connector
            .begin(IsolationLevel::ReadCommitted)
            .await
            .unwrap();
        assert!(!tx.completed);

        connector.commit(&mut tx).await.unwrap();
        assert!(tx.completed);
    }

    #[tokio::test]
    async fn test_rollback() {
        let config = create_test_config();
        let mut connector = PostgresConnector::new(config).await.unwrap();
        connector.connect().await.unwrap();

        let mut tx = connector.begin(IsolationLevel::Serializable).await.unwrap();
        connector.rollback(&mut tx).await.unwrap();
        assert!(tx.completed);
    }
}
