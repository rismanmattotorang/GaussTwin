use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server name
    pub name: String,
    /// Server version
    pub version: String,
    /// Environment (development, staging, production)
    pub environment: Environment,
    /// HTTP server configuration
    pub http: HttpConfig,
    /// gRPC server configuration
    pub grpc: GrpcConfig,
    /// GraphQL server configuration
    pub graphql: GraphQLConfig,
    /// WebSocket server configuration
    pub websocket: WebSocketConfig,
    /// Database configuration
    pub database: DatabaseConfig,
    /// Cache configuration
    pub cache: CacheConfig,
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Metrics configuration
    pub metrics: MetricsConfig,
    /// Milvus configuration
    pub milvus: MilvusConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Development,
    Staging,
    Production,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// HTTP server address
    pub addr: SocketAddr,
    /// Enable HTTPS
    pub enable_https: bool,
    /// TLS certificate path
    pub tls_cert_path: Option<PathBuf>,
    /// TLS key path
    pub tls_key_path: Option<PathBuf>,
    /// CORS configuration
    pub cors: CorsConfig,
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
    /// Request timeout in seconds
    pub timeout: u64,
    /// Maximum request body size in bytes
    pub max_body_size: usize,
    /// Enable compression
    pub enable_compression: bool,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:8080".parse().unwrap(),
            enable_https: false,
            tls_cert_path: None,
            tls_key_path: None,
            cors: CorsConfig::default(),
            rate_limit: RateLimitConfig::default(),
            timeout: 30,
            max_body_size: 1024 * 1024, // 1MB
            enable_compression: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    /// gRPC server address
    pub addr: SocketAddr,
    /// Enable TLS
    pub enable_tls: bool,
    /// TLS certificate path
    pub tls_cert_path: Option<PathBuf>,
    /// TLS key path
    pub tls_key_path: Option<PathBuf>,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Enable reflection service
    pub enable_reflection: bool,
    /// Enable health service
    pub enable_health: bool,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:9090".parse().unwrap(),
            enable_tls: false,
            tls_cert_path: None,
            tls_key_path: None,
            max_message_size: 1024 * 1024, // 1MB
            enable_reflection: true,
            enable_health: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLConfig {
    /// GraphQL endpoint path
    pub path: String,
    /// Enable GraphiQL interface
    pub enable_graphiql: bool,
    /// Maximum complexity
    pub max_complexity: Option<u32>,
    /// Maximum depth
    pub max_depth: Option<u32>,
    /// Enable subscriptions
    pub enable_subscriptions: bool,
    /// Enable introspection
    pub enable_introspection: bool,
}

impl Default for GraphQLConfig {
    fn default() -> Self {
        Self {
            path: "/graphql".to_string(),
            enable_graphiql: true,
            max_complexity: Some(1000),
            max_depth: Some(10),
            enable_subscriptions: true,
            enable_introspection: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// WebSocket endpoint path
    pub path: String,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u64,
    /// Client timeout in seconds
    pub client_timeout: u64,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Maximum connections per client
    pub max_connections: usize,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            path: "/ws".to_string(),
            heartbeat_interval: 30,
            client_timeout: 60,
            max_message_size: 1024 * 1024, // 1MB
            max_connections: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// SurrealDB configuration
    pub surrealdb: SurrealDBConfig,
    /// Enable database migrations
    pub enable_migrations: bool,
    /// Migration directory path
    pub migration_path: PathBuf,
    /// Connection pool size
    pub pool_size: u32,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            surrealdb: SurrealDBConfig::default(),
            enable_migrations: true,
            migration_path: PathBuf::from("migrations"),
            pool_size: 10,
            connection_timeout: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurrealDBConfig {
    /// Database URL
    pub url: String,
    /// Database namespace
    pub namespace: String,
    /// Database name
    pub database: String,
    /// Root username
    pub username: String,
    /// Root password
    pub password: String,
}

impl Default for SurrealDBConfig {
    fn default() -> Self {
        Self {
            url: "ws://localhost:8000".to_string(),
            namespace: "gausstwin".to_string(),
            database: "gausstwin".to_string(),
            username: "root".to_string(),
            password: "root".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// SkyTable configuration
    pub skytable: SkyTableConfig,
    /// Default TTL in seconds
    pub default_ttl: u64,
    /// Maximum cache size in bytes
    pub max_size: usize,
    /// Enable cache compression
    pub enable_compression: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            skytable: SkyTableConfig::default(),
            default_ttl: 3600,           // 1 hour
            max_size: 1024 * 1024 * 100, // 100MB
            enable_compression: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkyTableConfig {
    /// Cache host
    pub host: String,
    /// Cache port
    pub port: u16,
    /// Authentication token
    pub auth_token: Option<String>,
}

impl Default for SkyTableConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 2003,
            auth_token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// JWT secret key
    pub jwt_secret: String,
    /// JWT token expiration in seconds
    pub token_expiration: u64,
    /// Enable refresh tokens
    pub enable_refresh_tokens: bool,
    /// Refresh token expiration in seconds
    pub refresh_token_expiration: u64,
    /// Password hashing configuration
    pub password_hashing: PasswordHashingConfig,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: "your-secret-key-here".to_string(),
            token_expiration: 3600, // 1 hour
            enable_refresh_tokens: true,
            refresh_token_expiration: 86400, // 24 hours
            password_hashing: PasswordHashingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordHashingConfig {
    /// Memory cost
    pub memory_cost: u32,
    /// Time cost
    pub time_cost: u32,
    /// Parallelism
    pub parallelism: u32,
    /// Salt length
    pub salt_length: u32,
    /// Hash length
    pub hash_length: u32,
}

impl Default for PasswordHashingConfig {
    fn default() -> Self {
        Self {
            memory_cost: 65536,
            time_cost: 4,
            parallelism: 1,
            salt_length: 16,
            hash_length: 32,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable Prometheus metrics
    pub enable_prometheus: bool,
    /// Metrics endpoint path
    pub path: String,
    /// Collection interval in seconds
    pub interval: u64,
    /// Labels
    pub labels: std::collections::HashMap<String, String>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enable_prometheus: true,
            path: "/metrics".to_string(),
            interval: 15,
            labels: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilvusConfig {
    /// Milvus host
    pub host: String,
    /// Milvus port
    pub port: u16,
    /// Authentication token
    pub auth_token: Option<String>,
    /// Default collection name
    pub default_collection: String,
    /// Vector dimension
    pub dimension: u32,
    /// Index type
    pub index_type: String,
    /// Metric type
    pub metric_type: String,
}

impl Default for MilvusConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 19530,
            auth_token: None,
            default_collection: "gausstwin_vectors".to_string(),
            dimension: 128,
            index_type: "IVF_FLAT".to_string(),
            metric_type: "L2".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    /// Log format
    pub format: LogFormat,
    /// Log file path
    pub file_path: Option<PathBuf>,
    /// Enable JSON formatting
    pub json_format: bool,
    /// Enable request logging
    pub log_requests: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::Text,
            file_path: None,
            json_format: false,
            log_requests: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Text,
    Json,
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// Allowed origins
    pub allowed_origins: Vec<String>,
    /// Allowed methods
    pub allowed_methods: Vec<String>,
    /// Allowed headers
    pub allowed_headers: Vec<String>,
    /// Allow credentials
    pub allow_credentials: bool,
    /// Maximum age in seconds
    pub max_age: u64,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
            ],
            allowed_headers: vec!["*".to_string()],
            allow_credentials: true,
            max_age: 86400, // 24 hours
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per second
    pub requests_per_second: u32,
    /// Burst size
    pub burst_size: u32,
    /// Enable per-IP rate limiting
    pub per_ip: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 100,
            burst_size: 200,
            per_ip: true,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: "gausstwin-server".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: Environment::Development,
            http: HttpConfig::default(),
            grpc: GrpcConfig::default(),
            graphql: GraphQLConfig::default(),
            websocket: WebSocketConfig::default(),
            database: DatabaseConfig::default(),
            cache: CacheConfig::default(),
            auth: AuthConfig::default(),
            metrics: MetricsConfig::default(),
            milvus: MilvusConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

// All Default implementations are now manually defined above
