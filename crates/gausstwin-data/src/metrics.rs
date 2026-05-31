use metrics::{counter, gauge};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;

use crate::types::MetricsConfig;

/// Operation metrics for monitoring
#[derive(Debug, Default, Clone)]
pub struct OperationMetrics {
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub vector_operations: u64,
    pub db_operations: u64,
    pub vector_errors: u64,
    pub db_errors: u64,
    pub active_connections: u64,
    pub idle_connections: u64,
}

/// Metrics collector for monitoring
pub struct MetricsCollector {
    config: MetricsConfig,
    metrics: Arc<RwLock<OperationMetrics>>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(RwLock::new(OperationMetrics::default())),
        }
    }

    /// Record operation metrics
    pub fn record_operation(&self, operation: &str, duration: Duration, success: bool) {
        // Update the collector's own snapshot (consistent with record_*_operation),
        // so `get_metrics()` reflects recorded operations.
        {
            let mut metrics = self.metrics.write();
            metrics.total_operations += 1;
            if success {
                metrics.successful_operations += 1;
            } else {
                metrics.failed_operations += 1;
            }
        }

        let duration_ms = duration.as_millis() as f64;
        metrics::histogram!("data_operation_duration_ms", duration_ms, "operation" => operation.to_string());
        metrics::counter!("data_operation_total", 1, "operation" => operation.to_string(), "success" => success.to_string());
    }

    /// Record vector operation metrics
    pub fn record_vector_operation(&self, duration: Duration, success: bool) {
        let mut metrics = self.metrics.write();
        metrics.vector_operations += 1;

        if !success {
            metrics.vector_errors += 1;
        }

        counter!("data.vector.operations", 1);
        counter!("data.vector.latency", duration.as_millis() as u64);

        if !success {
            counter!("data.vector.errors", 1);
        }
    }

    /// Record database operation metrics
    pub fn record_db_operation(&self, duration: Duration, success: bool) {
        let mut metrics = self.metrics.write();
        metrics.db_operations += 1;

        if !success {
            metrics.db_errors += 1;
        }

        counter!("data.db.operations", 1);
        counter!("data.db.latency", duration.as_millis() as u64);

        if !success {
            counter!("data.db.errors", 1);
        }
    }

    /// Record connection pool metrics
    pub fn record_pool_metrics(&self, active: u64, idle: u64) {
        let mut metrics = self.metrics.write();
        metrics.active_connections = active;
        metrics.idle_connections = idle;

        gauge!("data.pool.connections.active", active as f64);
        gauge!("data.pool.connections.idle", idle as f64);
    }

    /// Get current metrics snapshot
    pub fn get_metrics(&self) -> OperationMetrics {
        self.metrics.read().clone()
    }
}

impl Clone for MetricsCollector {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            metrics: Arc::clone(&self.metrics),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_metrics_recording() {
        let collector = MetricsCollector::new(MetricsConfig {
            enabled: true,
            prefix: "test".into(),
            report_interval: Duration::from_secs(1),
        });

        // Record successful operation
        collector.record_operation("test", Duration::from_millis(100), true);
        let metrics = collector.get_metrics();
        assert_eq!(metrics.total_operations, 1);
        assert_eq!(metrics.successful_operations, 1);

        // Record failed operation
        collector.record_operation("test", Duration::from_millis(200), false);
        let metrics = collector.get_metrics();
        assert_eq!(metrics.total_operations, 2);
        assert_eq!(metrics.failed_operations, 1);

        // Record vector store operation
        collector.record_vector_operation(Duration::from_millis(300), true);
        let metrics = collector.get_metrics();
        assert_eq!(metrics.vector_operations, 1);

        // Record database operation
        collector.record_db_operation(Duration::from_millis(400), true);
        let metrics = collector.get_metrics();
        assert_eq!(metrics.db_operations, 1);
    }
}
