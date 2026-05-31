use crate::{AIError, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod config;
pub mod metrics;
pub mod state;
/// Core traits and types for AI components
pub mod traits;
pub mod types;

// Re-exports
pub use config::{AIConfig, ModelConfig, TrainingConfig};
pub use metrics::{InferenceMetrics, Metrics, TrainingMetrics};
pub use state::{AIState, ModelState};
pub use traits::{Agent, Environment};
pub use types::{Action, State};

/// Shared functionality for AI components
pub trait AIComponent: Send + Sync {
    /// Initialize the component
    async fn init(&mut self) -> Result<()>;

    /// Reset the component state
    async fn reset(&mut self) -> Result<()>;

    /// Update component state
    async fn update(&mut self) -> Result<()>;

    /// Get component metrics
    async fn get_metrics(&self) -> Result<Metrics>;

    /// Save component state
    async fn save(&self, path: &str) -> Result<()>;

    /// Load component state
    async fn load(&mut self, path: &str) -> Result<()>;
}

/// Shared state management
pub trait StateManager: Send + Sync {
    type State;

    /// Get current state
    async fn get_state(&self) -> Result<Self::State>;

    /// Set state
    async fn set_state(&mut self, state: Self::State) -> Result<()>;

    /// Update state
    async fn update_state<F, T>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Self::State) -> Result<T> + Send;
}

/// Configuration management
pub trait ConfigManager: Send + Sync {
    type Config: Clone;

    /// Get current configuration
    fn get_config(&self) -> &Self::Config;

    /// Update configuration
    fn update_config(&mut self, config: Self::Config) -> Result<()>;

    /// Validate configuration
    fn validate_config(&self, config: &Self::Config) -> Result<()>;
}

/// Resource management
pub struct ResourceManager {
    /// Available compute devices
    pub devices: Vec<String>,

    /// Memory limits
    pub memory_limits: std::collections::HashMap<String, usize>,

    /// Thread pool
    pub thread_pool: Arc<rayon::ThreadPool>,
}

impl ResourceManager {
    /// Create a new resource manager
    pub fn new() -> Result<Self> {
        let num_threads = num_cpus::get();
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .map_err(|e| {
                AIError::InitializationError(format!("Failed to create thread pool: {}", e))
            })?;

        Ok(Self {
            devices: vec!["cpu".to_string()], // Add GPU devices if available
            memory_limits: std::collections::HashMap::new(),
            thread_pool: Arc::new(thread_pool),
        })
    }

    /// Check if device is available
    pub fn is_device_available(&self, device: &str) -> bool {
        self.devices.contains(&device.to_string())
    }

    /// Get available memory for device
    pub fn get_available_memory(&self, device: &str) -> Option<usize> {
        self.memory_limits.get(device).copied()
    }

    /// Execute task in thread pool
    pub fn execute<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T + Send,
        T: Send,
    {
        self.thread_pool.install(f)
    }
}

/// Logging and monitoring
pub struct Monitor {
    /// Metrics history
    metrics: Arc<RwLock<Vec<Metrics>>>,

    /// Event subscribers
    subscribers: Vec<Box<dyn Fn(&Metrics) + Send + Sync>>,
}

impl Monitor {
    /// Create a new monitor
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Vec::new())),
            subscribers: Vec::new(),
        }
    }

    /// Add metrics
    pub async fn add_metrics(&mut self, metrics: Metrics) {
        let mut metrics_lock = self.metrics.write().await;
        metrics_lock.push(metrics.clone());

        // Notify subscribers
        for subscriber in &self.subscribers {
            subscriber(&metrics);
        }
    }

    /// Subscribe to metrics updates
    pub fn subscribe<F>(&mut self, callback: F)
    where
        F: Fn(&Metrics) + Send + Sync + 'static,
    {
        self.subscribers.push(Box::new(callback));
    }

    /// Get metrics history
    pub async fn get_metrics_history(&self) -> Vec<Metrics> {
        self.metrics.read().await.clone()
    }
}

/// Utility functions
pub mod utils {
    use super::*;

    /// Initialize logging
    pub fn init_logging() -> Result<()> {
        env_logger::init();
        Ok(())
    }

    /// Set up signal handlers
    pub fn setup_signal_handlers() -> Result<()> {
        ctrlc::set_handler(move || {
            log::info!("Received interrupt signal, shutting down...");
            std::process::exit(0);
        })
        .map_err(|e| {
            AIError::InitializationError(format!("Failed to set up signal handlers: {}", e))
        })
    }

    /// Generate unique ID
    pub fn generate_id() -> String {
        use uuid::Uuid;
        Uuid::new_v4().to_string()
    }

    /// Get timestamp
    pub fn get_timestamp() -> i64 {
        use chrono::Utc;
        Utc::now().timestamp()
    }
}
