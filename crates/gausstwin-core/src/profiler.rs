//! Performance Profiler Module
//!
//! Comprehensive profiling and performance analysis tools for GaussTwin simulations.
//!
//! # Features
//! - Hierarchical timing with automatic scope tracking
//! - Memory usage monitoring
//! - CPU utilization tracking
//! - Flame graph generation support
//! - Real-time metrics dashboard integration
//! - Thread-safe concurrent profiling
//! - Low-overhead sampling mode

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::ThreadId;
use std::time::{Duration, Instant};

/// Global profiler instance
/// Global master switch for profiling, toggled explicitly via
/// `PerformanceProfiler::set_enabled`. Defaults to enabled; per-instance
/// recording is additionally gated by each profiler's `config.enabled`.
static PROFILER_ENABLED: AtomicBool = AtomicBool::new(true);

/// Profiler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerConfig {
    /// Enable profiling
    pub enabled: bool,
    /// Sample rate (0.0-1.0, 1.0 = profile everything)
    pub sample_rate: f64,
    /// Maximum history entries per metric
    pub max_history: usize,
    /// Enable memory profiling
    pub memory_profiling: bool,
    /// Enable CPU profiling
    pub cpu_profiling: bool,
    /// Enable call graph generation
    pub call_graph: bool,
    /// Output format for reports
    pub output_format: OutputFormat,
    /// Minimum duration to record (microseconds)
    pub min_duration_us: u64,
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sample_rate: 1.0,
            max_history: 10000,
            memory_profiling: true,
            cpu_profiling: true,
            call_graph: false,
            output_format: OutputFormat::Json,
            min_duration_us: 0,
        }
    }
}

/// Output format for profiling reports
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFormat {
    /// JSON format
    Json,
    /// Human-readable text
    Text,
    /// Chrome tracing format (for chrome://tracing)
    ChromeTracing,
    /// Flame graph format (for flamegraph.pl)
    FlameGraph,
}

/// Timer statistics for a single metric
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimerStats {
    /// Total accumulated time
    pub total_time: Duration,
    /// Average time per call
    pub average_time: Duration,
    /// Minimum time observed
    pub min_time: Duration,
    /// Maximum time observed
    pub max_time: Duration,
    /// Number of calls
    pub call_count: u64,
    /// Standard deviation
    pub std_dev: Duration,
    /// Percentiles (p50, p90, p95, p99)
    pub percentiles: Percentiles,
}

/// Percentile statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Percentiles {
    pub p50: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub p99: Duration,
}

/// Memory snapshot data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    /// Timestamp
    pub timestamp: u64,
    /// Allocated heap memory (bytes)
    pub heap_allocated: usize,
    /// Active allocations count
    pub allocation_count: usize,
    /// Peak memory usage
    pub peak_usage: usize,
    /// Resident set size (if available)
    pub rss: Option<usize>,
    /// Virtual memory size (if available)
    pub vms: Option<usize>,
}

/// CPU usage data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuSnapshot {
    /// Timestamp
    pub timestamp: u64,
    /// CPU utilization (0.0-1.0)
    pub utilization: f64,
    /// User time
    pub user_time: Duration,
    /// System time
    pub system_time: Duration,
    /// Number of active threads
    pub thread_count: usize,
}

/// Call stack frame for call graph
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// Function/section name
    pub name: String,
    /// Start time (not serializable)
    #[allow(dead_code)]
    pub start_time: Instant,
    /// Duration (filled when frame ends)
    pub duration: Option<Duration>,
    /// Child frames
    pub children: Vec<StackFrame>,
    /// Thread ID
    pub thread_id: u64,
}

/// Thread-local timer data
struct TimerData {
    start_time: Option<Instant>,
    total_time: Duration,
    call_count: u64,
    samples: VecDeque<Duration>,
}

impl Default for TimerData {
    fn default() -> Self {
        Self {
            start_time: None,
            total_time: Duration::ZERO,
            call_count: 0,
            samples: VecDeque::new(),
        }
    }
}

/// Main performance profiler
pub struct PerformanceProfiler {
    /// Configuration
    config: ProfilerConfig,
    /// Timer data per metric name
    timers: RwLock<HashMap<String, TimerData>>,
    /// Memory snapshots
    memory_snapshots: RwLock<VecDeque<MemorySnapshot>>,
    /// CPU snapshots
    cpu_snapshots: RwLock<VecDeque<CpuSnapshot>>,
    /// Call stack (per thread)
    call_stacks: RwLock<HashMap<u64, Vec<String>>>,
    /// Global start time
    start_time: Instant,
    /// Total samples collected
    sample_count: AtomicU64,
    /// Hierarchical timers for nested measurements
    hierarchical_timers: RwLock<BTreeMap<String, HierarchicalTimer>>,
}

/// Hierarchical timer for nested scopes
#[derive(Debug, Clone)]
struct HierarchicalTimer {
    name: String,
    total_time: Duration,
    self_time: Duration,
    call_count: u64,
    children: HashMap<String, Box<HierarchicalTimer>>,
    samples: VecDeque<Duration>,
}

impl HierarchicalTimer {
    fn new(name: String) -> Self {
        Self {
            name,
            total_time: Duration::ZERO,
            self_time: Duration::ZERO,
            call_count: 0,
            children: HashMap::new(),
            samples: VecDeque::new(),
        }
    }
}

impl PerformanceProfiler {
    /// Create a new profiler with default configuration
    pub fn new() -> Self {
        Self::with_config(ProfilerConfig::default())
    }

    /// Create a new profiler with custom configuration
    pub fn with_config(config: ProfilerConfig) -> Self {
        // Note: construction does NOT mutate the global `PROFILER_ENABLED` master
        // switch. Whether a given profiler records is governed by its own
        // `config.enabled` (see `is_enabled`); the global is only changed via the
        // explicit `set_enabled`. This keeps independent profiler instances (and
        // concurrent tests) from interfering through shared global state.
        Self {
            config,
            timers: RwLock::new(HashMap::new()),
            memory_snapshots: RwLock::new(VecDeque::new()),
            cpu_snapshots: RwLock::new(VecDeque::new()),
            call_stacks: RwLock::new(HashMap::new()),
            start_time: Instant::now(),
            sample_count: AtomicU64::new(0),
            hierarchical_timers: RwLock::new(BTreeMap::new()),
        }
    }

    /// Check if profiling is enabled
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && PROFILER_ENABLED.load(Ordering::Relaxed)
    }

    /// Enable or disable profiling globally
    pub fn set_enabled(&self, enabled: bool) {
        PROFILER_ENABLED.store(enabled, Ordering::Relaxed);
    }

    /// Start timing a named section
    #[inline]
    pub fn start_timer(&self, name: &str) {
        if !self.is_enabled() {
            return;
        }

        // Sampling check
        if self.config.sample_rate < 1.0 {
            let sample = fastrand::f64();
            if sample >= self.config.sample_rate {
                return;
            }
        }

        let mut timers = self.timers.write();
        let timer = timers.entry(name.to_string()).or_default();
        timer.start_time = Some(Instant::now());

        // Track call stack
        if self.config.call_graph {
            let thread_id = thread_id_hash();
            let mut stacks = self.call_stacks.write();
            let stack = stacks.entry(thread_id).or_default();
            stack.push(name.to_string());
        }
    }

    /// Stop timing a named section
    #[inline]
    pub fn stop_timer(&self, name: &str) {
        if !self.is_enabled() {
            return;
        }

        let mut timers = self.timers.write();
        if let Some(timer) = timers.get_mut(name) {
            if let Some(start) = timer.start_time.take() {
                let elapsed = start.elapsed();

                // Skip if below minimum duration
                if elapsed.as_micros() as u64 >= self.config.min_duration_us {
                    timer.total_time += elapsed;
                    timer.call_count += 1;

                    // Store sample for percentile calculation
                    timer.samples.push_back(elapsed);
                    if timer.samples.len() > self.config.max_history {
                        timer.samples.pop_front();
                    }

                    self.sample_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Pop from call stack
        if self.config.call_graph {
            let thread_id = thread_id_hash();
            let mut stacks = self.call_stacks.write();
            if let Some(stack) = stacks.get_mut(&thread_id) {
                stack.pop();
            }
        }
    }

    /// Create a scoped timer that automatically stops when dropped
    pub fn scope(&self, name: &str) -> ProfileScope<'_> {
        ProfileScope::new(self, name)
    }

    /// Record a memory snapshot
    pub fn record_memory_snapshot(&self) {
        if !self.is_enabled() || !self.config.memory_profiling {
            return;
        }

        let snapshot = MemorySnapshot {
            timestamp: self.start_time.elapsed().as_micros() as u64,
            heap_allocated: get_heap_allocated(),
            allocation_count: 0, // Would need allocator hooks
            peak_usage: 0,
            rss: get_rss(),
            vms: get_vms(),
        };

        let mut snapshots = self.memory_snapshots.write();
        snapshots.push_back(snapshot);
        if snapshots.len() > self.config.max_history {
            snapshots.pop_front();
        }
    }

    /// Record a CPU usage snapshot
    pub fn record_cpu_snapshot(&self) {
        if !self.is_enabled() || !self.config.cpu_profiling {
            return;
        }

        let snapshot = CpuSnapshot {
            timestamp: self.start_time.elapsed().as_micros() as u64,
            utilization: get_cpu_utilization(),
            user_time: Duration::ZERO,
            system_time: Duration::ZERO,
            thread_count: get_thread_count(),
        };

        let mut snapshots = self.cpu_snapshots.write();
        snapshots.push_back(snapshot);
        if snapshots.len() > self.config.max_history {
            snapshots.pop_front();
        }
    }

    /// Get statistics for a specific timer
    pub fn get_timer_stats(&self, name: &str) -> Option<TimerStats> {
        let timers = self.timers.read();
        let timer = timers.get(name)?;

        if timer.call_count == 0 {
            return None;
        }

        let average_time = timer.total_time / timer.call_count as u32;

        // Calculate percentiles from samples
        let mut sorted_samples: Vec<Duration> = timer.samples.iter().copied().collect();
        sorted_samples.sort();

        let percentiles = if !sorted_samples.is_empty() {
            Percentiles {
                p50: percentile(&sorted_samples, 0.50),
                p90: percentile(&sorted_samples, 0.90),
                p95: percentile(&sorted_samples, 0.95),
                p99: percentile(&sorted_samples, 0.99),
            }
        } else {
            Percentiles::default()
        };

        // Calculate min/max
        let min_time = sorted_samples.first().copied().unwrap_or(Duration::ZERO);
        let max_time = sorted_samples.last().copied().unwrap_or(Duration::ZERO);

        // Calculate standard deviation
        let mean_ns = average_time.as_nanos() as f64;
        let variance: f64 = sorted_samples
            .iter()
            .map(|d| {
                let diff = d.as_nanos() as f64 - mean_ns;
                diff * diff
            })
            .sum::<f64>()
            / sorted_samples.len().max(1) as f64;
        let std_dev = Duration::from_nanos(variance.sqrt() as u64);

        Some(TimerStats {
            total_time: timer.total_time,
            average_time,
            min_time,
            max_time,
            call_count: timer.call_count,
            std_dev,
            percentiles,
        })
    }

    /// Get all timer statistics
    pub fn get_all_stats(&self) -> ProfilerStats {
        let timers = self.timers.read();
        let mut timer_stats = HashMap::new();

        for name in timers.keys() {
            if let Some(stats) = self.get_timer_stats(name) {
                timer_stats.insert(name.clone(), stats);
            }
        }

        // Get latest memory and CPU snapshots
        let memory_usage = {
            let snapshots = self.memory_snapshots.read();
            snapshots.back().map(|s| s.heap_allocated).unwrap_or(0)
        };

        let cpu_utilization = {
            let snapshots = self.cpu_snapshots.read();
            snapshots.back().map(|s| s.utilization).unwrap_or(0.0)
        };

        ProfilerStats {
            timer_stats,
            memory_usage,
            cpu_utilization,
            total_samples: self.sample_count.load(Ordering::Relaxed),
            profiler_overhead: self.estimate_overhead(),
            elapsed_time: self.start_time.elapsed(),
        }
    }

    /// Estimate profiler overhead
    fn estimate_overhead(&self) -> Duration {
        // Measure a single timing operation
        let start = Instant::now();
        for _ in 0..1000 {
            let _now = Instant::now();
        }
        start.elapsed() / 1000
    }

    /// Reset all statistics
    pub fn reset(&self) {
        self.timers.write().clear();
        self.memory_snapshots.write().clear();
        self.cpu_snapshots.write().clear();
        self.call_stacks.write().clear();
        self.hierarchical_timers.write().clear();
        self.sample_count.store(0, Ordering::Relaxed);
    }

    /// Generate a report in the configured format
    pub fn generate_report(&self) -> String {
        let stats = self.get_all_stats();

        match self.config.output_format {
            OutputFormat::Json => serde_json::to_string_pretty(&stats).unwrap_or_default(),
            OutputFormat::Text => self.format_text_report(&stats),
            OutputFormat::ChromeTracing => self.format_chrome_tracing(&stats),
            OutputFormat::FlameGraph => self.format_flame_graph(&stats),
        }
    }

    fn format_text_report(&self, stats: &ProfilerStats) -> String {
        let mut output = String::new();

        output.push_str("=== Performance Profile Report ===\n\n");
        output.push_str(&format!("Total elapsed time: {:?}\n", stats.elapsed_time));
        output.push_str(&format!("Total samples: {}\n", stats.total_samples));
        output.push_str(&format!("Memory usage: {} bytes\n", stats.memory_usage));
        output.push_str(&format!(
            "CPU utilization: {:.1}%\n",
            stats.cpu_utilization * 100.0
        ));
        output.push_str(&format!(
            "Profiler overhead: {:?}\n\n",
            stats.profiler_overhead
        ));

        output.push_str("Timer Statistics:\n");
        output.push_str(&"-".repeat(80));
        output.push('\n');

        // Sort by total time
        let mut timer_entries: Vec<_> = stats.timer_stats.iter().collect();
        timer_entries.sort_by(|a, b| b.1.total_time.cmp(&a.1.total_time));

        for (name, timer) in timer_entries {
            output.push_str(&format!(
                "{:<30} calls: {:>8} total: {:>12.3?} avg: {:>10.3?} p99: {:>10.3?}\n",
                name, timer.call_count, timer.total_time, timer.average_time, timer.percentiles.p99,
            ));
        }

        output
    }

    fn format_chrome_tracing(&self, _stats: &ProfilerStats) -> String {
        // Chrome tracing JSON format for chrome://tracing
        let mut events = Vec::new();
        let timers = self.timers.read();

        for (name, timer) in timers.iter() {
            if timer.call_count > 0 {
                events.push(serde_json::json!({
                    "name": name,
                    "cat": "profile",
                    "ph": "X",
                    "ts": 0,
                    "dur": timer.total_time.as_micros() / timer.call_count as u128,
                    "pid": 1,
                    "tid": 1,
                }));
            }
        }

        serde_json::to_string(&events).unwrap_or_default()
    }

    fn format_flame_graph(&self, _stats: &ProfilerStats) -> String {
        // Collapsed stack format for flamegraph.pl
        let mut output = String::new();
        let timers = self.timers.read();

        for (name, timer) in timers.iter() {
            if timer.call_count > 0 {
                output.push_str(&format!("{} {}\n", name, timer.total_time.as_micros()));
            }
        }

        output
    }

    /// Get memory history
    pub fn memory_history(&self) -> Vec<MemorySnapshot> {
        self.memory_snapshots.read().iter().cloned().collect()
    }

    /// Get CPU history
    pub fn cpu_history(&self) -> Vec<CpuSnapshot> {
        self.cpu_snapshots.read().iter().cloned().collect()
    }
}

impl Default for PerformanceProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for automatic scope timing
pub struct ProfileScope<'a> {
    profiler: &'a PerformanceProfiler,
    name: String,
}

impl<'a> ProfileScope<'a> {
    fn new(profiler: &'a PerformanceProfiler, name: &str) -> Self {
        profiler.start_timer(name);
        Self {
            profiler,
            name: name.to_string(),
        }
    }
}

impl<'a> Drop for ProfileScope<'a> {
    fn drop(&mut self) {
        self.profiler.stop_timer(&self.name);
    }
}

/// Complete profiler statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerStats {
    /// Statistics for each timer
    pub timer_stats: HashMap<String, TimerStats>,
    /// Current memory usage
    pub memory_usage: usize,
    /// Current CPU utilization
    pub cpu_utilization: f64,
    /// Total samples collected
    pub total_samples: u64,
    /// Estimated profiler overhead per measurement
    pub profiler_overhead: Duration,
    /// Total elapsed time
    pub elapsed_time: Duration,
}

// Helper functions

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let idx = ((sorted.len() as f64) * p) as usize;
    let idx = idx.min(sorted.len() - 1);
    sorted[idx]
}

fn thread_id_hash() -> u64 {
    // Simple hash of thread ID for HashMap key
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::thread::current().id().hash(&mut hasher);
    hasher.finish()
}

fn get_heap_allocated() -> usize {
    // Platform-specific implementation would go here
    // For now, return 0 as a placeholder
    0
}

fn get_rss() -> Option<usize> {
    // Platform-specific: get resident set size
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| s.split_whitespace().nth(1).map(|f| f.to_string()))
            .and_then(|s| s.parse::<usize>().ok())
            .map(|pages| pages * 4096) // Page size usually 4KB
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn get_vms() -> Option<usize> {
    // Platform-specific: get virtual memory size
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| s.split_whitespace().next().map(|f| f.to_string()))
            .and_then(|s| s.parse::<usize>().ok())
            .map(|pages| pages * 4096)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn get_cpu_utilization() -> f64 {
    // Would require tracking over time intervals
    // Placeholder implementation
    0.0
}

fn get_thread_count() -> usize {
    // Platform-specific implementation
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir("/proc/self/task")
            .map(|dir| dir.count())
            .unwrap_or(1)
    }
    #[cfg(not(target_os = "linux"))]
    {
        1
    }
}

/// Convenience macro for profiling a scope
#[macro_export]
macro_rules! profile_scope {
    ($profiler:expr, $name:expr) => {
        let _profile_guard = $profiler.scope($name);
    };
}

/// Convenience macro for profiling a function
#[macro_export]
macro_rules! profile_function {
    ($profiler:expr) => {
        let _profile_guard = $profiler.scope(concat!(module_path!(), "::", function_name!()));
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_basic_timing() {
        let profiler = PerformanceProfiler::new();

        profiler.start_timer("test_operation");
        thread::sleep(Duration::from_millis(10));
        profiler.stop_timer("test_operation");

        let stats = profiler.get_timer_stats("test_operation").unwrap();
        assert_eq!(stats.call_count, 1);
        assert!(stats.total_time >= Duration::from_millis(10));
    }

    #[test]
    fn test_scope_timing() {
        let profiler = PerformanceProfiler::new();

        {
            let _scope = profiler.scope("scoped_operation");
            thread::sleep(Duration::from_millis(5));
        }

        let stats = profiler.get_timer_stats("scoped_operation").unwrap();
        assert_eq!(stats.call_count, 1);
        assert!(stats.total_time >= Duration::from_millis(5));
    }

    #[test]
    fn test_multiple_calls() {
        let profiler = PerformanceProfiler::new();

        for _ in 0..10 {
            let _scope = profiler.scope("repeated_operation");
            thread::sleep(Duration::from_millis(1));
        }

        let stats = profiler.get_timer_stats("repeated_operation").unwrap();
        assert_eq!(stats.call_count, 10);
    }

    #[test]
    fn test_percentiles() {
        let profiler = PerformanceProfiler::new();

        for i in 0..100 {
            profiler.start_timer("percentile_test");
            thread::sleep(Duration::from_micros(100 + i * 10));
            profiler.stop_timer("percentile_test");
        }

        let stats = profiler.get_timer_stats("percentile_test").unwrap();
        assert!(stats.percentiles.p50 < stats.percentiles.p90);
        assert!(stats.percentiles.p90 < stats.percentiles.p99);
    }

    #[test]
    fn test_disabled_profiler() {
        let config = ProfilerConfig {
            enabled: false,
            ..Default::default()
        };
        let profiler = PerformanceProfiler::with_config(config);

        profiler.start_timer("should_not_record");
        thread::sleep(Duration::from_millis(10));
        profiler.stop_timer("should_not_record");

        assert!(profiler.get_timer_stats("should_not_record").is_none());
    }

    #[test]
    fn test_report_generation() {
        let profiler = PerformanceProfiler::new();

        {
            let _scope = profiler.scope("operation_a");
            thread::sleep(Duration::from_millis(5));
        }
        {
            let _scope = profiler.scope("operation_b");
            thread::sleep(Duration::from_millis(10));
        }

        let report = profiler.generate_report();
        assert!(!report.is_empty());
        assert!(report.contains("operation_a"));
        assert!(report.contains("operation_b"));
    }

    #[test]
    fn test_reset() {
        let profiler = PerformanceProfiler::new();

        {
            let _scope = profiler.scope("before_reset");
        }

        assert!(profiler.get_timer_stats("before_reset").is_some());

        profiler.reset();

        assert!(profiler.get_timer_stats("before_reset").is_none());
    }
}
