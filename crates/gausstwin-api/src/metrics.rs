use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use metrics::{counter, gauge, histogram, Key, KeyName, Unit, Label};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use crate::{config::MetricsConfig, error::{Error, Result}};

/// The Prometheus recorder is a process-wide singleton: it can be installed as
/// the global `metrics` recorder exactly once per process. We install it on the
/// first `MetricsManager::new` and cache the resulting handle so that subsequent
/// constructions (additional servers, test cases, re-initialization) reuse it
/// instead of failing with "a recorder has already been installed".
static GLOBAL_PROM_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
static INSTALL_LOCK: Mutex<()> = Mutex::new(());

/// Install the global Prometheus recorder once, returning a clone of the shared
/// handle. Uses double-checked locking so concurrent callers don't race on the
/// one-shot global install.
fn install_or_get_handle(config: &MetricsConfig) -> Result<PrometheusHandle> {
    if let Some(handle) = GLOBAL_PROM_HANDLE.get() {
        return Ok(handle.clone());
    }

    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Re-check now that we hold the lock: another caller may have installed it.
    if let Some(handle) = GLOBAL_PROM_HANDLE.get() {
        return Ok(handle.clone());
    }

    let builder = PrometheusBuilder::new();
    let builder = config
        .labels
        .iter()
        .fold(builder, |b, (k, v)| b.add_global_label(k, v));

    let handle = builder.install_recorder().map_err(|e| {
        Error::Configuration(format!("Failed to install metrics recorder: {}", e))
    })?;

    let _ = GLOBAL_PROM_HANDLE.set(handle.clone());
    Ok(handle)
}

/// Metrics manager for handling Prometheus metrics
pub struct MetricsManager {
    /// Prometheus handle
    handle: PrometheusHandle,
    /// Metrics configuration
    config: MetricsConfig,
}

impl MetricsManager {
    /// Create a new metrics manager
    pub fn new(config: &MetricsConfig) -> Result<Self> {
        let handle = install_or_get_handle(config)?;

        Ok(Self {
            handle,
            config: config.clone(),
        })
    }

    /// Get the Prometheus handle
    pub fn handle(&self) -> &PrometheusHandle {
        &self.handle
    }

    /// Render metrics
    pub fn render(&self) -> String {
        self.handle.render()
    }

    /// Record a counter increment
    pub fn increment_counter(&self, name: &str, value: u64, labels: Option<HashMap<String, String>>) {
        let key = self.create_key(name, labels);
        counter!(key.to_string(), value);
    }

    /// Record a gauge value
    pub fn set_gauge(&self, name: &str, value: f64, labels: Option<HashMap<String, String>>) {
        let key = self.create_key(name, labels);
        gauge!(key.to_string(), value);
    }

    /// Record a histogram value
    pub fn observe_histogram(&self, name: &str, value: f64, labels: Option<HashMap<String, String>>) {
        let key = self.create_key(name, labels);
        histogram!(key.to_string(), value);
    }

    /// Create a metric key with labels
    fn create_key(&self, name: &str, labels: Option<HashMap<String, String>>) -> Key {
        let mut key = Key::from_name(KeyName::from(name.to_string()));
        
        if let Some(labels) = labels {
            for (k, v) in labels {
                key = key.with_extra_labels(vec![Label::new(k, v)]);
            }
        }
        
        key
    }
}

/// HTTP metrics
pub struct HttpMetrics<'a> {
    manager: &'a MetricsManager,
}

impl<'a> HttpMetrics<'a> {
    /// Create new HTTP metrics
    pub fn new(manager: &'a MetricsManager) -> Self {
        Self { manager }
    }

    /// Record request
    pub fn record_request(&self, method: &str, path: &str, status: u16, duration: f64) {
        let labels = Some(HashMap::from([
            ("method".into(), method.into()),
            ("path".into(), path.into()),
            ("status".into(), status.to_string()),
        ]));

        // Record request count
        self.manager.increment_counter("http_requests_total", 1, labels.clone());

        // Record request duration
        self.manager.observe_histogram("http_request_duration_seconds", duration, labels);
    }

    /// Record error
    pub fn record_error(&self, method: &str, path: &str, error: &str) {
        let labels = Some(HashMap::from([
            ("method".into(), method.into()),
            ("path".into(), path.into()),
            ("error".into(), error.into()),
        ]));

        self.manager.increment_counter("http_errors_total", 1, labels);
    }
}

/// Database metrics
pub struct DatabaseMetrics<'a> {
    manager: &'a MetricsManager,
}

impl<'a> DatabaseMetrics<'a> {
    /// Create new database metrics
    pub fn new(manager: &'a MetricsManager) -> Self {
        Self { manager }
    }

    /// Record query
    pub fn record_query(&self, operation: &str, table: &str, duration: f64) {
        let labels = Some(HashMap::from([
            ("operation".into(), operation.into()),
            ("table".into(), table.into()),
        ]));

        // Record query count
        self.manager.increment_counter("db_queries_total", 1, labels.clone());

        // Record query duration
        self.manager.observe_histogram("db_query_duration_seconds", duration, labels);
    }

    /// Record error
    pub fn record_error(&self, operation: &str, table: &str, error: &str) {
        let labels = Some(HashMap::from([
            ("operation".into(), operation.into()),
            ("table".into(), table.into()),
            ("error".into(), error.into()),
        ]));

        self.manager.increment_counter("db_errors_total", 1, labels);
    }

    /// Record connection pool stats
    pub fn record_pool_stats(&self, active: u32, idle: u32, max_size: u32) {
        self.manager.set_gauge("db_connections_active", active as f64, None);
        self.manager.set_gauge("db_connections_idle", idle as f64, None);
        self.manager.set_gauge("db_connections_max", max_size as f64, None);
    }
}

/// Cache metrics
pub struct CacheMetrics<'a> {
    manager: &'a MetricsManager,
}

impl<'a> CacheMetrics<'a> {
    /// Create new cache metrics
    pub fn new(manager: &'a MetricsManager) -> Self {
        Self { manager }
    }

    /// Record operation
    pub fn record_operation(&self, operation: &str, hit: bool, duration: f64) {
        let labels = Some(HashMap::from([
            ("operation".into(), operation.into()),
            ("hit".into(), hit.to_string()),
        ]));

        // Record operation count
        self.manager.increment_counter("cache_operations_total", 1, labels.clone());

        // Record operation duration
        self.manager.observe_histogram("cache_operation_duration_seconds", duration, labels);
    }

    /// Record error
    pub fn record_error(&self, operation: &str, error: &str) {
        let labels = Some(HashMap::from([
            ("operation".into(), operation.into()),
            ("error".into(), error.into()),
        ]));

        self.manager.increment_counter("cache_errors_total", 1, labels);
    }

    /// Record cache stats
    pub fn record_stats(&self, size: u64, items: u64, hits: u64, misses: u64) {
        self.manager.set_gauge("cache_size_bytes", size as f64, None);
        self.manager.set_gauge("cache_items", items as f64, None);
        self.manager.set_gauge("cache_hits_total", hits as f64, None);
        self.manager.set_gauge("cache_misses_total", misses as f64, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_recording() {
        let config = MetricsConfig::default();
        let manager = MetricsManager::new(&config).unwrap();

        // Test counter
        manager.increment_counter("test_counter", 1, None);

        // Test gauge
        manager.set_gauge("test_gauge", 42.0, None);

        // Test histogram
        manager.observe_histogram("test_histogram", 0.5, None);

        // Verify metrics output
        let output = manager.render();
        assert!(output.contains("test_counter"));
        assert!(output.contains("test_gauge"));
        assert!(output.contains("test_histogram"));
    }
} 