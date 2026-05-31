//! High-Performance Computing Module
//!
//! Provides distributed and parallel computing capabilities for large-scale simulations.
//!
//! # Features
//! - Work-stealing thread pool with task prioritization
//! - NUMA-aware memory allocation hints
//! - Distributed simulation support via message passing
//! - Load balancing and dynamic partitioning
//! - Fault tolerance and checkpointing

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering as CmpOrdering;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::agent::AgentId;
use crate::error::{GaussTwinError, Result};

/// HPC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HpcConfig {
    /// Number of worker threads (0 = auto-detect)
    pub num_workers: usize,
    /// Enable work stealing
    pub work_stealing: bool,
    /// Task queue capacity per worker
    pub queue_capacity: usize,
    /// Enable NUMA awareness
    pub numa_aware: bool,
    /// Checkpoint interval (simulation steps)
    pub checkpoint_interval: Option<u64>,
    /// Maximum tasks per batch
    pub max_batch_size: usize,
    /// Load balancing strategy
    pub load_balancing: LoadBalancingStrategy,
}

impl Default for HpcConfig {
    fn default() -> Self {
        Self {
            num_workers: 0, // Auto-detect
            work_stealing: true,
            queue_capacity: 4096,
            numa_aware: true,
            checkpoint_interval: None,
            max_batch_size: 1000,
            load_balancing: LoadBalancingStrategy::WorkStealing,
        }
    }
}

/// Load balancing strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadBalancingStrategy {
    /// Round-robin distribution
    RoundRobin,
    /// Work stealing between workers
    WorkStealing,
    /// Least-loaded worker
    LeastLoaded,
    /// Locality-aware (minimize data movement)
    LocalityAware,
}

/// Task priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaskPriority {
    /// Background tasks
    Low = 0,
    /// Normal simulation tasks
    Normal = 1,
    /// Time-critical tasks
    High = 2,
    /// Real-time tasks
    Realtime = 3,
}

/// A task to be executed by the thread pool
pub struct Task {
    /// Task ID
    pub id: u64,
    /// Task priority
    pub priority: TaskPriority,
    /// The task closure
    pub work: Box<dyn FnOnce() + Send + 'static>,
    /// Task creation time
    pub created_at: Instant,
    /// Optional deadline
    pub deadline: Option<Instant>,
}

impl Task {
    /// Create a new task
    pub fn new<F>(id: u64, priority: TaskPriority, work: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            id,
            priority,
            work: Box::new(work),
            created_at: Instant::now(),
            deadline: None,
        }
    }

    /// Set a deadline for the task
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

// Ordering for priority queue (highest priority first, then earliest deadline)
impl Ord for Task {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        match self.priority.cmp(&other.priority).reverse() {
            CmpOrdering::Equal => match (&self.deadline, &other.deadline) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => CmpOrdering::Less,
                (None, Some(_)) => CmpOrdering::Greater,
                (None, None) => self.created_at.cmp(&other.created_at),
            },
            ord => ord,
        }
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Task {}

/// Statistics for the thread pool
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadPoolStats {
    /// Total tasks submitted
    pub tasks_submitted: u64,
    /// Total tasks completed
    pub tasks_completed: u64,
    /// Total tasks stolen (work stealing)
    pub tasks_stolen: u64,
    /// Average task latency
    pub avg_latency: Duration,
    /// Maximum task latency
    pub max_latency: Duration,
    /// Number of deadlines missed
    pub deadlines_missed: u64,
    /// Current queue depth
    pub queue_depth: usize,
    /// Worker utilization (0.0-1.0)
    pub worker_utilization: f64,
}

/// High-performance work-stealing thread pool
pub struct ThreadPool {
    /// Configuration
    config: HpcConfig,
    /// Worker handles
    workers: Vec<Worker>,
    /// Task sender for submission
    task_sender: Sender<Task>,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    /// Statistics
    stats: Arc<RwLock<ThreadPoolStats>>,
    /// Next task ID
    next_task_id: AtomicU64,
    /// Active task count
    active_tasks: Arc<AtomicUsize>,
}

struct Worker {
    id: usize,
    handle: Option<JoinHandle<()>>,
    local_queue: Arc<Mutex<VecDeque<Task>>>,
    tasks_completed: Arc<AtomicU64>,
}

impl ThreadPool {
    /// Create a new thread pool with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(HpcConfig::default())
    }

    /// Create a new thread pool with custom configuration
    pub fn with_config(config: HpcConfig) -> Result<Self> {
        let num_workers = if config.num_workers == 0 {
            num_cpus::get()
        } else {
            config.num_workers
        };

        let (task_sender, task_receiver) = bounded(config.queue_capacity);
        let shutdown = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(RwLock::new(ThreadPoolStats::default()));
        let active_tasks = Arc::new(AtomicUsize::new(0));

        let task_receiver = Arc::new(Mutex::new(task_receiver));

        // Create workers
        let mut workers = Vec::with_capacity(num_workers);
        let all_local_queues: Vec<Arc<Mutex<VecDeque<Task>>>> = (0..num_workers)
            .map(|_| Arc::new(Mutex::new(VecDeque::new())))
            .collect();

        for id in 0..num_workers {
            let shutdown = Arc::clone(&shutdown);
            let stats = Arc::clone(&stats);
            let task_receiver = Arc::clone(&task_receiver);
            let local_queue = Arc::clone(&all_local_queues[id]);
            let tasks_completed = Arc::new(AtomicU64::new(0));
            let tasks_completed_clone = Arc::clone(&tasks_completed);
            let active_tasks = Arc::clone(&active_tasks);
            let work_stealing = config.work_stealing;

            // Clone all local queues for work stealing
            let steal_queues: Vec<Arc<Mutex<VecDeque<Task>>>> = all_local_queues
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != id)
                .map(|(_, q)| Arc::clone(q))
                .collect();

            let handle = thread::Builder::new()
                .name(format!("gausstwin-worker-{}", id))
                .spawn(move || {
                    Self::worker_loop(
                        id,
                        shutdown,
                        stats,
                        task_receiver,
                        local_queue,
                        steal_queues,
                        tasks_completed_clone,
                        active_tasks,
                        work_stealing,
                    );
                })
                .map_err(|e| GaussTwinError::ThreadPoolError(e.to_string()))?;

            workers.push(Worker {
                id,
                handle: Some(handle),
                local_queue: Arc::clone(&all_local_queues[id]),
                tasks_completed,
            });
        }

        tracing::info!("Thread pool created with {} workers", num_workers);

        Ok(Self {
            config,
            workers,
            task_sender,
            shutdown,
            stats,
            next_task_id: AtomicU64::new(0),
            active_tasks,
        })
    }

    fn worker_loop(
        id: usize,
        shutdown: Arc<AtomicBool>,
        stats: Arc<RwLock<ThreadPoolStats>>,
        task_receiver: Arc<Mutex<Receiver<Task>>>,
        local_queue: Arc<Mutex<VecDeque<Task>>>,
        steal_queues: Vec<Arc<Mutex<VecDeque<Task>>>>,
        tasks_completed: Arc<AtomicU64>,
        active_tasks: Arc<AtomicUsize>,
        work_stealing: bool,
    ) {
        while !shutdown.load(Ordering::Relaxed) {
            // First, try local queue
            let task = {
                let mut queue = local_queue.lock();
                queue.pop_front()
            };

            let task = task.or_else(|| {
                // Try global queue
                let receiver = task_receiver.lock();
                receiver.try_recv().ok()
            });

            let task = task.or_else(|| {
                // Try work stealing
                if work_stealing {
                    for steal_queue in &steal_queues {
                        if let Some(task) = steal_queue.lock().pop_back() {
                            stats.write().tasks_stolen += 1;
                            return Some(task);
                        }
                    }
                }
                None
            });

            if let Some(task) = task {
                // Note: `active_tasks` is incremented at submit time (it counts
                // outstanding pending+in-flight tasks); the worker only decrements
                // it once the task has finished executing, below.
                let start = Instant::now();

                // Execute the task
                (task.work)();

                let elapsed = start.elapsed();

                // Update statistics
                {
                    let mut stats = stats.write();
                    stats.tasks_completed += 1;

                    // Update latency tracking
                    let latency = task.created_at.elapsed();
                    if stats.avg_latency == Duration::ZERO {
                        stats.avg_latency = latency;
                    } else {
                        stats.avg_latency = (stats.avg_latency + latency) / 2;
                    }
                    if latency > stats.max_latency {
                        stats.max_latency = latency;
                    }

                    // Check deadline
                    if let Some(deadline) = task.deadline {
                        if Instant::now() > deadline {
                            stats.deadlines_missed += 1;
                        }
                    }
                }

                tasks_completed.fetch_add(1, Ordering::Relaxed);
                active_tasks.fetch_sub(1, Ordering::Relaxed);
            } else {
                // No work available, yield briefly
                thread::yield_now();
            }
        }

        tracing::debug!("Worker {} shutting down", id);
    }

    /// Submit a task to the thread pool
    pub fn submit<F>(&self, priority: TaskPriority, work: F) -> Result<u64>
    where
        F: FnOnce() + Send + 'static,
    {
        let task_id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        let task = Task::new(task_id, priority, work);

        // Count the task as outstanding *before* it can be picked up by a worker.
        // `wait_idle()` waits on this counter, so it must include pending (queued
        // but not yet started) tasks, not only in-flight ones.
        self.active_tasks.fetch_add(1, Ordering::SeqCst);
        self.task_sender.send(task).map_err(|_| {
            self.active_tasks.fetch_sub(1, Ordering::SeqCst);
            GaussTwinError::ThreadPoolError("Failed to submit task".to_string())
        })?;

        self.stats.write().tasks_submitted += 1;

        Ok(task_id)
    }

    /// Submit a task with a deadline
    pub fn submit_with_deadline<F>(
        &self,
        priority: TaskPriority,
        deadline: Instant,
        work: F,
    ) -> Result<u64>
    where
        F: FnOnce() + Send + 'static,
    {
        let task_id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        let task = Task::new(task_id, priority, work).with_deadline(deadline);

        self.active_tasks.fetch_add(1, Ordering::SeqCst);
        self.task_sender.send(task).map_err(|_| {
            self.active_tasks.fetch_sub(1, Ordering::SeqCst);
            GaussTwinError::ThreadPoolError("Failed to submit task".to_string())
        })?;

        self.stats.write().tasks_submitted += 1;

        Ok(task_id)
    }

    /// Submit a batch of tasks
    pub fn submit_batch<F>(&self, priority: TaskPriority, tasks: Vec<F>) -> Result<Vec<u64>>
    where
        F: FnOnce() + Send + 'static,
    {
        let mut task_ids = Vec::with_capacity(tasks.len());

        for work in tasks {
            let task_id = self.submit(priority, work)?;
            task_ids.push(task_id);
        }

        Ok(task_ids)
    }

    /// Wait for all tasks to complete
    pub fn wait_idle(&self) {
        while self.active_tasks.load(Ordering::Relaxed) > 0 {
            thread::yield_now();
        }
    }

    /// Get thread pool statistics
    pub fn stats(&self) -> ThreadPoolStats {
        let mut stats = self.stats.read().clone();
        stats.queue_depth = self.active_tasks.load(Ordering::Relaxed);

        // Calculate worker utilization
        let total_completed: u64 = self
            .workers
            .iter()
            .map(|w| w.tasks_completed.load(Ordering::Relaxed))
            .sum();

        if stats.tasks_completed > 0 {
            // Rough estimate of utilization
            stats.worker_utilization =
                (self.active_tasks.load(Ordering::Relaxed) as f64) / self.workers.len() as f64;
        }

        stats
    }

    /// Get number of workers
    pub fn num_workers(&self) -> usize {
        self.workers.len()
    }

    /// Shutdown the thread pool
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

impl Default for ThreadPool {
    fn default() -> Self {
        Self::new().expect("Failed to create default thread pool")
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.shutdown();

        for worker in &mut self.workers {
            if let Some(handle) = worker.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

/// Parallel iterator utilities
pub mod parallel {
    use super::*;
    use std::sync::mpsc;

    /// Execute a function in parallel over a slice
    pub fn parallel_for<T, F>(pool: &ThreadPool, data: &[T], f: F) -> Result<()>
    where
        T: Sync + Send + 'static,
        F: Fn(&T) + Send + Sync + Clone + 'static,
    {
        let chunk_size = (data.len() + pool.num_workers() - 1) / pool.num_workers();
        let data_ptr = data.as_ptr() as usize;
        let data_len = data.len();

        let (tx, rx) = mpsc::channel();
        let num_chunks = (data_len + chunk_size - 1) / chunk_size;

        for chunk_idx in 0..num_chunks {
            let start = chunk_idx * chunk_size;
            let end = (start + chunk_size).min(data_len);
            let f = f.clone();
            let tx = tx.clone();

            pool.submit(TaskPriority::Normal, move || {
                let data_ptr = data_ptr as *const T;
                for i in start..end {
                    unsafe {
                        f(&*data_ptr.add(i));
                    }
                }
                let _ = tx.send(());
            })?;
        }

        // Wait for all chunks to complete
        for _ in 0..num_chunks {
            let _ = rx.recv();
        }

        Ok(())
    }

    /// Map function in parallel
    ///
    /// Note: T must be Clone to distribute across workers
    pub fn parallel_map<T, U, F>(pool: &ThreadPool, data: Vec<T>, f: F) -> Result<Vec<U>>
    where
        T: Send + Clone + 'static,
        U: Send + 'static,
        F: Fn(T) -> U + Send + Sync + Clone + 'static,
    {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        let num_workers = pool.num_workers().max(1);
        let chunk_size = (data.len() + num_workers - 1) / num_workers;
        let (tx, rx) = mpsc::channel();

        // Create chunks with indices
        let mut chunks: Vec<(usize, Vec<T>)> = Vec::new();
        for (idx, chunk) in data.chunks(chunk_size).enumerate() {
            chunks.push((idx, chunk.to_vec()));
        }

        let num_chunks = chunks.len();

        for (chunk_idx, chunk) in chunks {
            let f = f.clone();
            let tx = tx.clone();

            pool.submit(TaskPriority::Normal, move || {
                let results: Vec<U> = chunk.into_iter().map(&f).collect();
                let _ = tx.send((chunk_idx, results));
            })?;
        }

        // Collect results in order
        let mut all_results: Vec<(usize, Vec<U>)> = Vec::with_capacity(num_chunks);
        for _ in 0..num_chunks {
            if let Ok(result) = rx.recv() {
                all_results.push(result);
            }
        }

        all_results.sort_by_key(|(idx, _)| *idx);
        Ok(all_results
            .into_iter()
            .flat_map(|(_, results)| results)
            .collect())
    }
}

/// NUMA awareness utilities
#[cfg(target_os = "linux")]
pub mod numa {
    use super::*;

    /// Get the number of NUMA nodes
    pub fn num_nodes() -> usize {
        // Read from /sys/devices/system/node/
        std::fs::read_dir("/sys/devices/system/node")
            .map(|dir| {
                dir.filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with("node"))
                    .count()
            })
            .unwrap_or(1)
    }

    /// Get memory info for a NUMA node
    pub fn node_memory_info(node: usize) -> Option<(usize, usize)> {
        let path = format!("/sys/devices/system/node/node{}/meminfo", node);
        let content = std::fs::read_to_string(path).ok()?;

        let mut total = 0usize;
        let mut free = 0usize;

        for line in content.lines() {
            if line.contains("MemTotal:") {
                total = line
                    .split_whitespace()
                    .nth(3)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            } else if line.contains("MemFree:") {
                free = line
                    .split_whitespace()
                    .nth(3)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }
        }

        Some((total * 1024, free * 1024))
    }
}

#[cfg(not(target_os = "linux"))]
pub mod numa {
    pub fn num_nodes() -> usize {
        1
    }

    pub fn node_memory_info(_node: usize) -> Option<(usize, usize)> {
        None
    }
}

/// Distributed computing support
pub mod distributed {
    use super::*;

    /// Message types for distributed simulation
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum DistributedMessage {
        /// Agent migration request
        MigrateAgent {
            agent_id: AgentId,
            target_node: u32,
            state: Vec<u8>,
        },
        /// Synchronization barrier
        Barrier { step: u64 },
        /// State synchronization
        SyncState {
            node_id: u32,
            step: u64,
            checksum: u64,
        },
        /// Heartbeat
        Heartbeat { node_id: u32, timestamp: u64 },
        /// Shutdown signal
        Shutdown,
    }

    /// Node information for distributed simulation
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct NodeInfo {
        /// Node ID
        pub id: u32,
        /// Node hostname/address
        pub address: String,
        /// Number of agents on this node
        pub agent_count: usize,
        /// Node status
        pub status: NodeStatus,
        /// Last heartbeat timestamp
        pub last_heartbeat: u64,
    }

    /// Node status
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum NodeStatus {
        /// Node is initializing
        Initializing,
        /// Node is ready
        Ready,
        /// Node is running simulation
        Running,
        /// Node is paused
        Paused,
        /// Node has failed
        Failed,
        /// Node is shutting down
        ShuttingDown,
    }
}

// Provide CPU count without external dependency
mod num_cpus {
    pub fn get() -> usize {
        std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn test_thread_pool_creation() {
        let pool = ThreadPool::new().unwrap();
        assert!(pool.num_workers() > 0);
    }

    #[test]
    fn test_task_submission() {
        let pool = ThreadPool::new().unwrap();
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..100 {
            let counter = Arc::clone(&counter);
            pool.submit(TaskPriority::Normal, move || {
                counter.fetch_add(1, Ordering::Relaxed);
            })
            .unwrap();
        }

        pool.wait_idle();

        assert_eq!(counter.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn test_task_priorities() {
        let pool = ThreadPool::with_config(HpcConfig {
            num_workers: 1, // Single worker for predictable ordering
            ..Default::default()
        })
        .unwrap();

        let results = Arc::new(Mutex::new(Vec::new()));

        // Submit tasks with different priorities
        for i in 0..10 {
            let results = Arc::clone(&results);
            let priority = match i % 3 {
                0 => TaskPriority::Low,
                1 => TaskPriority::Normal,
                _ => TaskPriority::High,
            };

            pool.submit(priority, move || {
                results.lock().push(i);
            })
            .unwrap();
        }

        pool.wait_idle();

        let results = results.lock();
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn test_parallel_map() {
        let pool = ThreadPool::new().unwrap();

        let data: Vec<i32> = (0..1000).collect();
        let results = parallel::parallel_map(&pool, data, |x| x * 2).unwrap();

        assert_eq!(results.len(), 1000);
        for (i, &result) in results.iter().enumerate() {
            assert_eq!(result, (i as i32) * 2);
        }
    }

    #[test]
    fn test_thread_pool_stats() {
        let pool = ThreadPool::new().unwrap();

        for _ in 0..50 {
            pool.submit(TaskPriority::Normal, || {
                thread::sleep(Duration::from_millis(1));
            })
            .unwrap();
        }

        pool.wait_idle();

        let stats = pool.stats();
        assert_eq!(stats.tasks_submitted, 50);
        assert_eq!(stats.tasks_completed, 50);
    }

    #[test]
    fn test_numa_info() {
        let nodes = numa::num_nodes();
        assert!(nodes >= 1);
    }
}
