//! Agent Pool Module
//!
//! High-performance object pooling for agent instances to minimize allocations
//! and improve cache locality. Implements lock-free operations where possible.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use crossbeam_queue::ArrayQueue;

use crate::agent::{Agent, AgentId, AgentState};
use crate::error::{GaussTwinError, Result};

/// Pool statistics for monitoring and optimization
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    /// Total number of objects created
    pub total_created: u64,
    /// Total number of objects acquired from pool
    pub total_acquired: u64,
    /// Total number of objects returned to pool
    pub total_returned: u64,
    /// Total number of objects destroyed
    pub total_destroyed: u64,
    /// Current pool size (available objects)
    pub current_size: usize,
    /// Peak pool size
    pub peak_size: usize,
    /// Number of cache hits (object reused)
    pub cache_hits: u64,
    /// Number of cache misses (new allocation required)
    pub cache_misses: u64,
}

impl PoolStats {
    /// Calculate cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }
    
    /// Calculate reuse rate
    pub fn reuse_rate(&self) -> f64 {
        if self.total_acquired == 0 {
            0.0
        } else {
            self.total_returned as f64 / self.total_acquired as f64
        }
    }
}

/// Configuration for object pools
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Initial pool capacity
    pub initial_capacity: usize,
    /// Maximum pool capacity (0 for unlimited)
    pub max_capacity: usize,
    /// Whether to pre-warm the pool
    pub pre_warm: bool,
    /// Enable statistics collection
    pub collect_stats: bool,
    /// Shrink threshold (fraction of capacity)
    pub shrink_threshold: f64,
    /// Growth factor when expanding
    pub growth_factor: f64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            initial_capacity: 1024,
            max_capacity: 0, // Unlimited
            pre_warm: false,
            collect_stats: true,
            shrink_threshold: 0.25,
            growth_factor: 2.0,
        }
    }
}

/// Lock-free object pool using crossbeam's ArrayQueue
/// 
/// This implementation provides:
/// - O(1) acquire and release operations
/// - Lock-free concurrent access
/// - Bounded memory usage
/// - Statistics collection
pub struct ObjectPool<T> {
    /// Internal storage using lock-free queue
    storage: ArrayQueue<T>,
    /// Factory function to create new objects
    factory: Box<dyn Fn() -> T + Send + Sync>,
    /// Reset function to prepare object for reuse
    reset: Box<dyn Fn(&mut T) + Send + Sync>,
    /// Pool configuration
    config: PoolConfig,
    /// Statistics
    stats: Arc<PoolStatsAtomic>,
}

/// Atomic statistics for lock-free updates
struct PoolStatsAtomic {
    total_created: AtomicU64,
    total_acquired: AtomicU64,
    total_returned: AtomicU64,
    total_destroyed: AtomicU64,
    current_size: AtomicUsize,
    peak_size: AtomicUsize,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
}

impl Default for PoolStatsAtomic {
    fn default() -> Self {
        Self {
            total_created: AtomicU64::new(0),
            total_acquired: AtomicU64::new(0),
            total_returned: AtomicU64::new(0),
            total_destroyed: AtomicU64::new(0),
            current_size: AtomicUsize::new(0),
            peak_size: AtomicUsize::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        }
    }
}

impl<T: Send> ObjectPool<T> {
    /// Create a new object pool with custom factory and reset functions
    pub fn new<F, R>(factory: F, reset: R, config: PoolConfig) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
        R: Fn(&mut T) + Send + Sync + 'static,
    {
        let capacity = if config.max_capacity > 0 {
            config.max_capacity
        } else {
            config.initial_capacity.max(1024)
        };
        
        let pool = Self {
            storage: ArrayQueue::new(capacity),
            factory: Box::new(factory),
            reset: Box::new(reset),
            config,
            stats: Arc::new(PoolStatsAtomic::default()),
        };
        
        // Pre-warm the pool if configured
        if pool.config.pre_warm {
            for _ in 0..pool.config.initial_capacity {
                let obj = (pool.factory)();
                let _ = pool.storage.push(obj);
                pool.stats.total_created.fetch_add(1, Ordering::Relaxed);
                pool.stats.current_size.fetch_add(1, Ordering::Relaxed);
            }
            pool.update_peak_size();
        }
        
        pool
    }
    
    /// Acquire an object from the pool
    /// 
    /// Returns an existing object if available, otherwise creates a new one.
    /// This operation is lock-free.
    pub fn acquire(&self) -> T {
        self.stats.total_acquired.fetch_add(1, Ordering::Relaxed);
        
        match self.storage.pop() {
            Some(mut obj) => {
                self.stats.cache_hits.fetch_add(1, Ordering::Relaxed);
                self.stats.current_size.fetch_sub(1, Ordering::Relaxed);
                (self.reset)(&mut obj);
                obj
            }
            None => {
                self.stats.cache_misses.fetch_add(1, Ordering::Relaxed);
                self.stats.total_created.fetch_add(1, Ordering::Relaxed);
                (self.factory)()
            }
        }
    }
    
    /// Release an object back to the pool
    /// 
    /// Returns true if the object was successfully added to the pool,
    /// false if the pool is at capacity (object will be dropped).
    pub fn release(&self, obj: T) -> bool {
        self.stats.total_returned.fetch_add(1, Ordering::Relaxed);
        
        match self.storage.push(obj) {
            Ok(()) => {
                self.stats.current_size.fetch_add(1, Ordering::Relaxed);
                self.update_peak_size();
                true
            }
            Err(_) => {
                // Pool is full, object will be dropped
                self.stats.total_destroyed.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
    
    /// Get the current number of available objects
    pub fn available(&self) -> usize {
        self.stats.current_size.load(Ordering::Relaxed)
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            total_created: self.stats.total_created.load(Ordering::Relaxed),
            total_acquired: self.stats.total_acquired.load(Ordering::Relaxed),
            total_returned: self.stats.total_returned.load(Ordering::Relaxed),
            total_destroyed: self.stats.total_destroyed.load(Ordering::Relaxed),
            current_size: self.stats.current_size.load(Ordering::Relaxed),
            peak_size: self.stats.peak_size.load(Ordering::Relaxed),
            cache_hits: self.stats.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.stats.cache_misses.load(Ordering::Relaxed),
        }
    }
    
    /// Clear the pool, destroying all objects
    pub fn clear(&self) {
        while let Some(_) = self.storage.pop() {
            self.stats.total_destroyed.fetch_add(1, Ordering::Relaxed);
            self.stats.current_size.fetch_sub(1, Ordering::Relaxed);
        }
    }
    
    fn update_peak_size(&self) {
        let current = self.stats.current_size.load(Ordering::Relaxed);
        let mut peak = self.stats.peak_size.load(Ordering::Relaxed);
        
        while current > peak {
            match self.stats.peak_size.compare_exchange_weak(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(p) => peak = p,
            }
        }
    }
}

/// RAII guard for automatic pool return
pub struct PoolGuard<'a, T: Send> {
    pool: &'a ObjectPool<T>,
    obj: Option<T>,
}

impl<'a, T: Send> PoolGuard<'a, T> {
    /// Create a new pool guard
    pub fn new(pool: &'a ObjectPool<T>) -> Self {
        Self {
            pool,
            obj: Some(pool.acquire()),
        }
    }
    
    /// Take ownership of the object (prevents auto-return)
    pub fn take(mut self) -> T {
        self.obj.take().expect("Object already taken")
    }
}

impl<'a, T: Send> std::ops::Deref for PoolGuard<'a, T> {
    type Target = T;
    
    fn deref(&self) -> &Self::Target {
        self.obj.as_ref().expect("Object already taken")
    }
}

impl<'a, T: Send> std::ops::DerefMut for PoolGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.obj.as_mut().expect("Object already taken")
    }
}

impl<'a, T: Send> Drop for PoolGuard<'a, T> {
    fn drop(&mut self) {
        if let Some(obj) = self.obj.take() {
            self.pool.release(obj);
        }
    }
}

/// Typed agent pool with lifecycle management
pub struct AgentPool<S: AgentState> {
    /// Internal object pool for agent containers
    inner: ObjectPool<AgentContainer<S>>,
    /// Active agent registry
    active_agents: RwLock<std::collections::HashMap<AgentId, usize>>,
    /// Generation counter for versioning
    generation: AtomicU64,
}

/// Container for pooled agents
pub struct AgentContainer<S: AgentState> {
    /// Agent ID (may be reused)
    pub id: AgentId,
    /// Agent state
    pub state: S,
    /// Generation number for validation
    pub generation: u64,
    /// Whether the agent is active
    pub active: bool,
}

impl<S: AgentState + Default> AgentPool<S> {
    /// Create a new agent pool
    pub fn new(config: PoolConfig) -> Self {
        let inner = ObjectPool::new(
            || AgentContainer {
                id: AgentId::new(),
                state: S::default(),
                generation: 0,
                active: false,
            },
            |container| {
                container.active = false;
                // State is kept for reuse, will be reset when activated
            },
            config,
        );
        
        Self {
            inner,
            active_agents: RwLock::new(std::collections::HashMap::new()),
            generation: AtomicU64::new(0),
        }
    }
    
    /// Acquire an agent from the pool
    pub fn acquire_agent(&self) -> AgentContainer<S> {
        let mut container = self.inner.acquire();
        container.generation = self.generation.fetch_add(1, Ordering::Relaxed);
        container.active = true;
        container.id = AgentId::new(); // Generate new ID
        container
    }
    
    /// Release an agent back to the pool
    pub fn release_agent(&self, mut container: AgentContainer<S>) {
        container.active = false;
        self.inner.release(container);
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        self.inner.stats()
    }
}

/// Memory arena for bulk agent allocations
/// 
/// Provides contiguous memory allocation for agents to improve
/// cache performance during iteration.
pub struct AgentArena<S: AgentState> {
    /// Backing storage
    storage: Vec<Option<AgentContainer<S>>>,
    /// Free slot indices
    free_slots: VecDeque<usize>,
    /// Active count
    active_count: usize,
    /// Capacity
    capacity: usize,
}

impl<S: AgentState + Default + Clone> AgentArena<S> {
    /// Create a new arena with specified capacity
    pub fn new(capacity: usize) -> Self {
        let mut storage = Vec::with_capacity(capacity);
        storage.resize_with(capacity, || None);
        
        let free_slots: VecDeque<usize> = (0..capacity).collect();
        
        Self {
            storage,
            free_slots,
            active_count: 0,
            capacity,
        }
    }
    
    /// Allocate an agent in the arena
    pub fn allocate(&mut self) -> Result<usize> {
        match self.free_slots.pop_front() {
            Some(slot) => {
                self.storage[slot] = Some(AgentContainer {
                    id: AgentId::new(),
                    state: S::default(),
                    generation: 0,
                    active: true,
                });
                self.active_count += 1;
                Ok(slot)
            }
            None => Err(GaussTwinError::CapacityExceeded(
                format!("Arena capacity {} exceeded", self.capacity)
            ))
        }
    }
    
    /// Deallocate an agent from the arena
    pub fn deallocate(&mut self, slot: usize) -> Result<()> {
        if slot >= self.capacity {
            return Err(GaussTwinError::IndexOutOfBounds(slot, self.capacity));
        }
        
        if self.storage[slot].is_some() {
            self.storage[slot] = None;
            // Reuse the most-recently-freed slot first (LIFO/stack discipline).
            // This keeps freshly-freed slots hot in cache and gives deterministic
            // immediate reuse after a deallocation.
            self.free_slots.push_front(slot);
            self.active_count -= 1;
        }
        
        Ok(())
    }
    
    /// Get reference to agent at slot
    pub fn get(&self, slot: usize) -> Option<&AgentContainer<S>> {
        if slot < self.capacity {
            self.storage[slot].as_ref()
        } else {
            None
        }
    }
    
    /// Get mutable reference to agent at slot
    pub fn get_mut(&mut self, slot: usize) -> Option<&mut AgentContainer<S>> {
        if slot < self.capacity {
            self.storage[slot].as_mut()
        } else {
            None
        }
    }
    
    /// Iterate over all active agents
    pub fn iter(&self) -> impl Iterator<Item = (usize, &AgentContainer<S>)> {
        self.storage.iter().enumerate().filter_map(|(i, opt)| {
            opt.as_ref().map(|container| (i, container))
        })
    }
    
    /// Iterate mutably over all active agents
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut AgentContainer<S>)> {
        self.storage.iter_mut().enumerate().filter_map(|(i, opt)| {
            opt.as_mut().map(|container| (i, container))
        })
    }
    
    /// Get active agent count
    pub fn active_count(&self) -> usize {
        self.active_count
    }
    
    /// Get arena capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    
    /// Check if arena is full
    pub fn is_full(&self) -> bool {
        self.active_count >= self.capacity
    }
    
    /// Clear all agents from the arena
    pub fn clear(&mut self) {
        for i in 0..self.capacity {
            if self.storage[i].is_some() {
                self.storage[i] = None;
                self.free_slots.push_back(i);
            }
        }
        self.active_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::DefaultAgentState;
    
    #[test]
    fn test_object_pool_basic() {
        let pool: ObjectPool<Vec<u8>> = ObjectPool::new(
            || Vec::with_capacity(1024),
            |v| v.clear(),
            PoolConfig::default(),
        );
        
        // Acquire an object
        let mut v1 = pool.acquire();
        v1.push(1);
        v1.push(2);
        
        // Release it
        pool.release(v1);
        
        // Acquire again - should get the same (reset) object
        let v2 = pool.acquire();
        assert!(v2.is_empty()); // Reset function cleared it
        
        let stats = pool.stats();
        assert_eq!(stats.cache_hits, 1); // Second acquire hit the cache
        assert_eq!(stats.cache_misses, 1); // First acquire was a miss
    }
    
    #[test]
    fn test_pool_guard() {
        let pool: ObjectPool<Vec<u8>> = ObjectPool::new(
            || Vec::with_capacity(1024),
            |v| v.clear(),
            PoolConfig::default(),
        );
        
        {
            let mut guard = PoolGuard::new(&pool);
            guard.push(1);
            guard.push(2);
            assert_eq!(guard.len(), 2);
        } // Guard drops here, returning to pool
        
        let stats = pool.stats();
        assert_eq!(stats.total_returned, 1);
    }
    
    #[test]
    fn test_agent_pool() {
        let pool: AgentPool<DefaultAgentState> = AgentPool::new(PoolConfig::default());
        
        let agent1 = pool.acquire_agent();
        let id1 = agent1.id;
        assert!(agent1.active);
        
        pool.release_agent(agent1);
        
        let agent2 = pool.acquire_agent();
        assert!(agent2.active);
        assert_ne!(agent2.id, id1); // New ID assigned
        
        let stats = pool.stats();
        assert_eq!(stats.total_acquired, 2);
    }
    
    #[test]
    fn test_agent_arena() {
        let mut arena: AgentArena<DefaultAgentState> = AgentArena::new(100);
        
        // Allocate some agents
        let slot1 = arena.allocate().unwrap();
        let slot2 = arena.allocate().unwrap();
        let slot3 = arena.allocate().unwrap();
        
        assert_eq!(arena.active_count(), 3);
        
        // Deallocate one
        arena.deallocate(slot2).unwrap();
        assert_eq!(arena.active_count(), 2);
        
        // Allocate another - should reuse slot
        let slot4 = arena.allocate().unwrap();
        assert_eq!(slot4, slot2); // Reused slot
        assert_eq!(arena.active_count(), 3);
        
        // Iterate
        let count = arena.iter().count();
        assert_eq!(count, 3);
    }
    
    #[test]
    fn test_arena_capacity() {
        let mut arena: AgentArena<DefaultAgentState> = AgentArena::new(2);
        
        arena.allocate().unwrap();
        arena.allocate().unwrap();
        
        // Should fail - capacity exceeded
        assert!(arena.allocate().is_err());
        assert!(arena.is_full());
    }
    
    #[test]
    fn test_pool_stats() {
        let pool: ObjectPool<Vec<u8>> = ObjectPool::new(
            || Vec::with_capacity(1024),
            |v| v.clear(),
            PoolConfig::default(),
        );
        
        // Create some activity
        for _ in 0..10 {
            let v = pool.acquire();
            pool.release(v);
        }
        
        let stats = pool.stats();
        assert_eq!(stats.total_acquired, 10);
        assert_eq!(stats.total_returned, 10);
        assert!(stats.hit_rate() > 0.8); // Most should be cache hits
    }
}
