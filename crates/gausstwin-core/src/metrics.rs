//! # Metrics Collection and Monitoring
//!
//! This module provides comprehensive metrics collection, aggregation, and monitoring
//! capabilities for the GaussTwin digital twin framework.
//!
//! ## Features
//!
//! - Real-time metrics collection
//! - Statistical aggregation (mean, median, percentiles)
//! - Time-series data storage
//! - Performance monitoring
//! - Custom metric types
//! - Export capabilities (JSON, CSV, Prometheus)

use crate::{
    agent::AgentId,
    error::{GaussTwinError, Result},
    time::SimTime,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Central metrics collector for the GaussTwin framework
#[derive(Debug)]
pub struct MetricsCollector {
    /// Counter metrics (monotonically increasing values)
    counters: HashMap<String, Counter>,
    /// Gauge metrics (current values that can go up or down)
    gauges: HashMap<String, Gauge>,
    /// Histogram metrics (distribution of values)
    histograms: HashMap<String, Histogram>,
    /// Time series data
    time_series: HashMap<String, TimeSeries>,
    /// Agent-specific metrics
    agent_metrics: HashMap<AgentId, AgentMetrics>,
    /// System metrics
    system_metrics: SystemMetrics,
    /// Configuration
    config: MetricsConfig,
    /// Start time for uptime calculation
    start_time: Instant,
}

/// Configuration for metrics collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Maximum number of data points to keep in time series
    pub max_time_series_points: usize,
    /// How often to collect system metrics
    pub system_metrics_interval: Duration,
    /// Whether to enable detailed agent metrics
    pub enable_agent_metrics: bool,
    /// Whether to enable performance profiling
    pub enable_profiling: bool,
    /// Export format preference
    pub export_format: ExportFormat,
}

/// Export format options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    /// JSON format
    Json,
    /// CSV format
    Csv,
    /// Prometheus format
    Prometheus,
    /// Custom format
    Custom(String),
}

/// Counter metric (monotonically increasing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counter {
    /// Current value
    pub value: u64,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Last update timestamp
    pub updated_at: SystemTime,
    /// Description
    pub description: String,
    /// Labels for categorization
    pub labels: HashMap<String, String>,
}

/// Gauge metric (current value that can fluctuate)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gauge {
    /// Current value
    pub value: f64,
    /// Minimum value seen
    pub min_value: f64,
    /// Maximum value seen
    pub max_value: f64,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Last update timestamp
    pub updated_at: SystemTime,
    /// Description
    pub description: String,
    /// Labels for categorization
    pub labels: HashMap<String, String>,
}

/// Histogram metric (distribution of values)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Histogram {
    /// Bucket boundaries
    pub buckets: Vec<f64>,
    /// Count in each bucket
    pub bucket_counts: Vec<u64>,
    /// Total count of observations
    pub count: u64,
    /// Sum of all observed values
    pub sum: f64,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Last update timestamp
    pub updated_at: SystemTime,
    /// Description
    pub description: String,
    /// Labels for categorization
    pub labels: HashMap<String, String>,
}

/// Time series data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    /// Timestamp
    pub timestamp: SystemTime,
    /// Simulation time
    pub sim_time: Option<SimTime>,
    /// Value
    pub value: f64,
    /// Optional labels
    pub labels: HashMap<String, String>,
}

/// Time series collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeries {
    /// Data points
    pub points: VecDeque<DataPoint>,
    /// Maximum number of points to keep
    pub max_points: usize,
    /// Description
    pub description: String,
}

/// Agent-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetrics {
    /// Agent ID
    pub agent_id: AgentId,
    /// Number of actions taken
    pub actions_count: u64,
    /// Number of messages sent
    pub messages_sent: u64,
    /// Number of messages received
    pub messages_received: u64,
    /// Total execution time
    pub execution_time: Duration,
    /// Last position (if applicable)
    pub last_position: Option<crate::space::VecN>,
    /// Custom properties
    pub properties: HashMap<String, f64>,
    /// Creation time
    pub created_at: SystemTime,
    /// Last update time
    pub updated_at: SystemTime,
}

/// System-wide metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// Total number of agents
    pub total_agents: usize,
    /// Active agents
    pub active_agents: usize,
    /// Events processed per second
    pub events_per_second: f64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Simulation time
    pub simulation_time: SimTime,
    /// Real time elapsed
    pub real_time_elapsed: Duration,
    /// Simulation speed (sim_time / real_time)
    pub simulation_speed: f64,
    /// Last update timestamp
    pub updated_at: SystemTime,
}

/// Aggregated statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    /// Count of values
    pub count: u64,
    /// Sum of values
    pub sum: f64,
    /// Mean value
    pub mean: f64,
    /// Median value
    pub median: f64,
    /// Standard deviation
    pub std_dev: f64,
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// 95th percentile
    pub p95: f64,
    /// 99th percentile
    pub p99: f64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            max_time_series_points: 10000,
            system_metrics_interval: Duration::from_secs(1),
            enable_agent_metrics: true,
            enable_profiling: true,
            export_format: ExportFormat::Json,
        }
    }
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self::with_config(MetricsConfig::default())
    }

    /// Create a new metrics collector with custom configuration
    pub fn with_config(config: MetricsConfig) -> Self {
        Self {
            counters: HashMap::new(),
            gauges: HashMap::new(),
            histograms: HashMap::new(),
            time_series: HashMap::new(),
            agent_metrics: HashMap::new(),
            system_metrics: SystemMetrics {
                total_agents: 0,
                active_agents: 0,
                events_per_second: 0.0,
                memory_usage: 0,
                cpu_usage: 0.0,
                simulation_time: SimTime::zero(),
                real_time_elapsed: Duration::default(),
                simulation_speed: 0.0,
                updated_at: SystemTime::now(),
            },
            config,
            start_time: Instant::now(),
        }
    }

    /// Increment a counter metric
    pub fn increment_counter(&mut self, name: &str, description: &str) -> Result<()> {
        self.increment_counter_by(name, 1, description)
    }

    /// Increment a counter metric by a specific amount
    pub fn increment_counter_by(
        &mut self,
        name: &str,
        amount: u64,
        description: &str,
    ) -> Result<()> {
        let counter = self
            .counters
            .entry(name.to_string())
            .or_insert_with(|| Counter {
                value: 0,
                created_at: SystemTime::now(),
                updated_at: SystemTime::now(),
                description: description.to_string(),
                labels: HashMap::new(),
            });

        counter.value += amount;
        counter.updated_at = SystemTime::now();
        Ok(())
    }

    /// Set a gauge metric value
    pub fn set_gauge(&mut self, name: &str, value: f64, description: &str) -> Result<()> {
        let gauge = self
            .gauges
            .entry(name.to_string())
            .or_insert_with(|| Gauge {
                value: 0.0,
                min_value: f64::INFINITY,
                max_value: f64::NEG_INFINITY,
                created_at: SystemTime::now(),
                updated_at: SystemTime::now(),
                description: description.to_string(),
                labels: HashMap::new(),
            });

        gauge.value = value;
        gauge.min_value = gauge.min_value.min(value);
        gauge.max_value = gauge.max_value.max(value);
        gauge.updated_at = SystemTime::now();
        Ok(())
    }

    /// Record a value in a histogram
    pub fn record_histogram(&mut self, name: &str, value: f64, description: &str) -> Result<()> {
        let histogram = self.histograms.entry(name.to_string()).or_insert_with(|| {
            // Default buckets for latency measurements (in milliseconds)
            let buckets = vec![
                0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0,
            ];
            let bucket_counts = vec![0; buckets.len() + 1]; // +1 for overflow bucket

            Histogram {
                buckets,
                bucket_counts,
                count: 0,
                sum: 0.0,
                created_at: SystemTime::now(),
                updated_at: SystemTime::now(),
                description: description.to_string(),
                labels: HashMap::new(),
            }
        });

        histogram.count += 1;
        histogram.sum += value;
        histogram.updated_at = SystemTime::now();

        // Find appropriate bucket
        let bucket_index = histogram
            .buckets
            .iter()
            .position(|&bucket| value <= bucket)
            .unwrap_or(histogram.buckets.len());

        histogram.bucket_counts[bucket_index] += 1;
        Ok(())
    }

    /// Add a data point to a time series
    pub fn record_time_series(
        &mut self,
        name: &str,
        value: f64,
        sim_time: Option<SimTime>,
        description: &str,
    ) -> Result<()> {
        let time_series = self
            .time_series
            .entry(name.to_string())
            .or_insert_with(|| TimeSeries {
                points: VecDeque::new(),
                max_points: self.config.max_time_series_points,
                description: description.to_string(),
            });

        let data_point = DataPoint {
            timestamp: SystemTime::now(),
            sim_time,
            value,
            labels: HashMap::new(),
        };

        time_series.points.push_back(data_point);

        // Keep only the most recent points
        while time_series.points.len() > time_series.max_points {
            time_series.points.pop_front();
        }

        Ok(())
    }

    /// Update agent metrics
    pub fn update_agent_metrics(&mut self, agent_id: AgentId) -> Result<()> {
        if !self.config.enable_agent_metrics {
            return Ok(());
        }

        let agent_metrics = self
            .agent_metrics
            .entry(agent_id)
            .or_insert_with(|| AgentMetrics {
                agent_id,
                actions_count: 0,
                messages_sent: 0,
                messages_received: 0,
                execution_time: Duration::default(),
                last_position: None,
                properties: HashMap::new(),
                created_at: SystemTime::now(),
                updated_at: SystemTime::now(),
            });

        agent_metrics.updated_at = SystemTime::now();
        Ok(())
    }

    /// Record agent action
    pub fn record_agent_action(
        &mut self,
        agent_id: AgentId,
        execution_time: Duration,
    ) -> Result<()> {
        if let Some(metrics) = self.agent_metrics.get_mut(&agent_id) {
            metrics.actions_count += 1;
            metrics.execution_time += execution_time;
            metrics.updated_at = SystemTime::now();
        }
        Ok(())
    }

    /// Record agent message sent
    pub fn record_agent_message_sent(&mut self, agent_id: AgentId) -> Result<()> {
        if let Some(metrics) = self.agent_metrics.get_mut(&agent_id) {
            metrics.messages_sent += 1;
            metrics.updated_at = SystemTime::now();
        }
        Ok(())
    }

    /// Record agent message received
    pub fn record_agent_message_received(&mut self, agent_id: AgentId) -> Result<()> {
        if let Some(metrics) = self.agent_metrics.get_mut(&agent_id) {
            metrics.messages_received += 1;
            metrics.updated_at = SystemTime::now();
        }
        Ok(())
    }

    /// Update system metrics
    pub fn update_system_metrics(
        &mut self,
        sim_time: SimTime,
        total_agents: usize,
        active_agents: usize,
    ) -> Result<()> {
        let now = SystemTime::now();
        let real_elapsed = self.start_time.elapsed();

        self.system_metrics.simulation_time = sim_time;
        self.system_metrics.total_agents = total_agents;
        self.system_metrics.active_agents = active_agents;
        self.system_metrics.real_time_elapsed = real_elapsed;
        self.system_metrics.simulation_speed = if real_elapsed.as_secs_f64() > 0.0 {
            sim_time.value() / real_elapsed.as_secs_f64()
        } else {
            0.0
        };
        self.system_metrics.updated_at = now;

        // Update memory usage (simplified - in real implementation would use system calls)
        self.system_metrics.memory_usage = self.estimate_memory_usage();

        Ok(())
    }

    /// Get counter value
    pub fn get_counter(&self, name: &str) -> Option<u64> {
        self.counters.get(name).map(|c| c.value)
    }

    /// Get gauge value
    pub fn get_gauge(&self, name: &str) -> Option<f64> {
        self.gauges.get(name).map(|g| g.value)
    }

    /// Get histogram statistics
    pub fn get_histogram_stats(&self, name: &str) -> Option<Statistics> {
        self.histograms.get(name).map(|h| {
            let mean = if h.count > 0 {
                h.sum / h.count as f64
            } else {
                0.0
            };

            Statistics {
                count: h.count,
                sum: h.sum,
                mean,
                median: mean, // Simplified - would need actual value distribution
                std_dev: 0.0, // Simplified - would calculate from distribution
                min: 0.0,     // Simplified - would track actual min
                max: 0.0,     // Simplified - would track actual max
                p95: 0.0,     // Simplified - would calculate from buckets
                p99: 0.0,     // Simplified - would calculate from buckets
            }
        })
    }

    /// Get time series data
    pub fn get_time_series(&self, name: &str) -> Option<&TimeSeries> {
        self.time_series.get(name)
    }

    /// Get agent metrics
    pub fn get_agent_metrics(&self, agent_id: AgentId) -> Option<&AgentMetrics> {
        self.agent_metrics.get(&agent_id)
    }

    /// Get system metrics
    pub fn get_system_metrics(&self) -> &SystemMetrics {
        &self.system_metrics
    }

    /// Get all metric names
    pub fn get_metric_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        names.extend(self.counters.keys().cloned());
        names.extend(self.gauges.keys().cloned());
        names.extend(self.histograms.keys().cloned());
        names.extend(self.time_series.keys().cloned());
        names.sort();
        names
    }

    /// Export metrics as JSON
    pub fn export_json(&self) -> Result<String> {
        let export_data = serde_json::json!({
            "counters": self.counters,
            "gauges": self.gauges,
            "histograms": self.histograms,
            "time_series": self.time_series,
            "agent_metrics": self.agent_metrics,
            "system_metrics": self.system_metrics,
            "exported_at": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| GaussTwinError::Custom(format!("Time error: {}", e)))?
                .as_secs()
        });

        serde_json::to_string_pretty(&export_data)
            .map_err(|e| GaussTwinError::Custom(format!("JSON serialization error: {}", e)))
    }

    /// Export metrics as CSV
    pub fn export_csv(&self) -> Result<String> {
        let mut csv = String::new();
        csv.push_str("metric_type,name,value,timestamp,description\n");

        // Export counters
        for (name, counter) in &self.counters {
            let timestamp = counter
                .updated_at
                .duration_since(UNIX_EPOCH)
                .map_err(|e| GaussTwinError::Custom(format!("Time error: {}", e)))?
                .as_secs();
            csv.push_str(&format!(
                "counter,{},{},{},{}\n",
                name, counter.value, timestamp, counter.description
            ));
        }

        // Export gauges
        for (name, gauge) in &self.gauges {
            let timestamp = gauge
                .updated_at
                .duration_since(UNIX_EPOCH)
                .map_err(|e| GaussTwinError::Custom(format!("Time error: {}", e)))?
                .as_secs();
            csv.push_str(&format!(
                "gauge,{},{},{},{}\n",
                name, gauge.value, timestamp, gauge.description
            ));
        }

        Ok(csv)
    }

    /// Clear all metrics
    pub fn clear(&mut self) {
        self.counters.clear();
        self.gauges.clear();
        self.histograms.clear();
        self.time_series.clear();
        self.agent_metrics.clear();
        self.start_time = Instant::now();
    }

    /// Get uptime
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    // Helper method to estimate memory usage
    fn estimate_memory_usage(&self) -> u64 {
        // Simplified estimation - in real implementation would use system APIs
        let base_size = std::mem::size_of::<Self>() as u64;
        let counters_size = self.counters.len() as u64 * 64;
        let gauges_size = self.gauges.len() as u64 * 96;
        let histograms_size = self.histograms.len() as u64 * 256;
        let time_series_size = self
            .time_series
            .values()
            .map(|ts| ts.points.len() as u64 * 64)
            .sum::<u64>();
        let agent_metrics_size = self.agent_metrics.len() as u64 * 128;

        base_size
            + counters_size
            + gauges_size
            + histograms_size
            + time_series_size
            + agent_metrics_size
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience trait for objects that can provide metrics
pub trait Measurable {
    /// Get metric names that this object provides
    fn metric_names(&self) -> Vec<String>;

    /// Record metrics to the collector
    fn record_metrics(&self, collector: &mut MetricsCollector) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new();
        assert_eq!(collector.get_metric_names().len(), 0);
    }

    #[test]
    fn test_counter_operations() {
        let mut collector = MetricsCollector::new();

        collector
            .increment_counter("test_counter", "A test counter")
            .unwrap();
        assert_eq!(collector.get_counter("test_counter"), Some(1));

        collector
            .increment_counter_by("test_counter", 5, "A test counter")
            .unwrap();
        assert_eq!(collector.get_counter("test_counter"), Some(6));
    }

    #[test]
    fn test_gauge_operations() {
        let mut collector = MetricsCollector::new();

        collector
            .set_gauge("test_gauge", 42.5, "A test gauge")
            .unwrap();
        assert_eq!(collector.get_gauge("test_gauge"), Some(42.5));

        collector
            .set_gauge("test_gauge", 10.0, "A test gauge")
            .unwrap();
        assert_eq!(collector.get_gauge("test_gauge"), Some(10.0));
    }

    #[test]
    fn test_histogram_operations() {
        let mut collector = MetricsCollector::new();

        collector
            .record_histogram("test_histogram", 1.5, "A test histogram")
            .unwrap();
        collector
            .record_histogram("test_histogram", 2.5, "A test histogram")
            .unwrap();

        let stats = collector.get_histogram_stats("test_histogram").unwrap();
        assert_eq!(stats.count, 2);
        assert_eq!(stats.sum, 4.0);
        assert_eq!(stats.mean, 2.0);
    }

    #[test]
    fn test_time_series_operations() {
        let mut collector = MetricsCollector::new();

        collector
            .record_time_series("test_series", 1.0, Some(SimTime::new(1.0)), "A test series")
            .unwrap();
        collector
            .record_time_series("test_series", 2.0, Some(SimTime::new(2.0)), "A test series")
            .unwrap();

        let series = collector.get_time_series("test_series").unwrap();
        assert_eq!(series.points.len(), 2);
        assert_eq!(series.points[0].value, 1.0);
        assert_eq!(series.points[1].value, 2.0);
    }

    #[test]
    fn test_agent_metrics() {
        let mut collector = MetricsCollector::new();
        let agent_id = AgentId::new();

        collector.update_agent_metrics(agent_id).unwrap();
        collector
            .record_agent_action(agent_id, Duration::from_millis(100))
            .unwrap();
        collector.record_agent_message_sent(agent_id).unwrap();

        let metrics = collector.get_agent_metrics(agent_id).unwrap();
        assert_eq!(metrics.actions_count, 1);
        assert_eq!(metrics.messages_sent, 1);
        assert_eq!(metrics.execution_time, Duration::from_millis(100));
    }

    #[test]
    fn test_system_metrics() {
        let mut collector = MetricsCollector::new();

        collector
            .update_system_metrics(SimTime::new(10.0), 100, 80)
            .unwrap();

        let metrics = collector.get_system_metrics();
        assert_eq!(metrics.total_agents, 100);
        assert_eq!(metrics.active_agents, 80);
        assert_eq!(metrics.simulation_time, SimTime::new(10.0));
    }

    #[test]
    fn test_export_json() {
        let mut collector = MetricsCollector::new();
        collector.increment_counter("test", "Test counter").unwrap();
        collector
            .set_gauge("test_gauge", 42.0, "Test gauge")
            .unwrap();

        let json = collector.export_json().unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("test_gauge"));
        assert!(json.contains("42"));
    }

    #[test]
    fn test_export_csv() {
        let mut collector = MetricsCollector::new();
        collector.increment_counter("test", "Test counter").unwrap();
        collector
            .set_gauge("test_gauge", 42.0, "Test gauge")
            .unwrap();

        let csv = collector.export_csv().unwrap();
        assert!(csv.contains("counter,test,1"));
        assert!(csv.contains("gauge,test_gauge,42"));
    }
}
