use async_trait::async_trait;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::error::DataResult;
use crate::types::CacheConfig;

/// Cache entry with metadata
#[derive(Clone)]
struct CacheEntry<V> {
    value: V,
    created_at: Instant,
    last_access: Instant,
    access_count: u64,
    ttl: Option<Duration>,
}

/// Cache statistics
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub size: usize,
    pub max_size: usize,
}

/// Async cache trait
#[async_trait]
pub trait AsyncCache<K, V>: Send + Sync {
    /// Get a value from the cache
    async fn get(&self, key: &K) -> DataResult<Option<V>>;

    /// Put a value into the cache
    async fn put(&self, key: K, value: V, ttl: Option<Duration>) -> DataResult<()>;

    /// Remove a value from the cache
    async fn remove(&self, key: &K) -> DataResult<()>;

    /// Clear all entries from the cache
    async fn clear(&self) -> DataResult<()>;

    /// Get cache statistics
    async fn get_stats(&self) -> CacheStats;
}

/// LRU cache implementation
pub struct LruCache<K, V> {
    entries: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,
    stats: Arc<RwLock<CacheStats>>,
    config: CacheConfig,
}

impl<K: Clone + Hash + Eq + Send + Sync + 'static, V: Clone + Send + Sync + 'static>
    LruCache<K, V>
{
    /// Create a new LRU cache
    pub fn new(config: CacheConfig) -> Self {
        let stats = Arc::new(RwLock::new(CacheStats {
            hits: 0,
            misses: 0,
            evictions: 0,
            size: 0,
            max_size: config.max_size,
        }));

        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            stats,
            config,
        }
    }

    async fn update_stats(&self, hit: bool) {
        let mut stats = self.stats.write().await;
        if hit {
            stats.hits += 1;
        } else {
            stats.misses += 1;
        }
    }

    async fn evict_expired(&self) {
        let mut entries = self.entries.write().await;
        let mut stats = self.stats.write().await;
        let now = Instant::now();

        let mut to_remove = Vec::new();
        for (key, entry) in entries.iter() {
            if let Some(ttl) = entry.ttl {
                if now.duration_since(entry.created_at) > ttl {
                    to_remove.push(key.clone());
                }
            }
        }

        for key in to_remove {
            entries.remove(&key);
            stats.evictions += 1;
            stats.size = stats.size.saturating_sub(1);
        }
    }

    async fn evict_lru(&self) {
        let mut entries = self.entries.write().await;
        let mut stats = self.stats.write().await;

        if entries.len() > self.config.max_size {
            let to_evict = entries.len() - self.config.max_size;
            let keys: Vec<_> = entries.keys().take(to_evict).cloned().collect();
            for key in keys {
                entries.remove(&key);
                stats.evictions += 1;
                stats.size = stats.size.saturating_sub(1);
            }
        }
    }
}

#[async_trait]
impl<K: Clone + Hash + Eq + Send + Sync + 'static, V: Clone + Send + Sync + 'static>
    AsyncCache<K, V> for LruCache<K, V>
{
    async fn get(&self, key: &K) -> DataResult<Option<V>> {
        self.evict_expired().await;

        let entries = self.entries.read().await;
        let entry = entries.get(key);

        match entry {
            Some(entry) => {
                let now = Instant::now();
                if let Some(ttl) = entry.ttl {
                    if now.duration_since(entry.created_at) > ttl {
                        self.update_stats(false).await;
                        return Ok(None);
                    }
                }
                self.update_stats(true).await;
                Ok(Some(entry.value.clone()))
            }
            None => {
                self.update_stats(false).await;
                Ok(None)
            }
        }
    }

    async fn put(&self, key: K, value: V, ttl: Option<Duration>) -> DataResult<()> {
        let mut entries = self.entries.write().await;
        let mut stats = self.stats.write().await;

        entries.insert(
            key,
            CacheEntry {
                value,
                created_at: Instant::now(),
                last_access: Instant::now(),
                access_count: 0,
                ttl,
            },
        );

        // Evict overflow entries inline using the locks we already hold. Calling
        // `evict_lru()` here would re-acquire these same write locks and deadlock
        // (tokio's RwLock is not reentrant).
        if entries.len() > self.config.max_size {
            let to_evict = entries.len() - self.config.max_size;
            let keys: Vec<_> = entries.keys().take(to_evict).cloned().collect();
            for key in keys {
                entries.remove(&key);
                stats.evictions += 1;
            }
        }

        stats.size = entries.len();

        Ok(())
    }

    async fn remove(&self, key: &K) -> DataResult<()> {
        let mut entries = self.entries.write().await;
        let mut stats = self.stats.write().await;

        if entries.remove(key).is_some() {
            stats.size = stats.size.saturating_sub(1);
        }

        Ok(())
    }

    async fn clear(&self) -> DataResult<()> {
        let mut entries = self.entries.write().await;
        let mut stats = self.stats.write().await;

        entries.clear();
        stats.size = 0;

        Ok(())
    }

    async fn get_stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }
}

impl<K, V> Clone for LruCache<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            entries: Arc::clone(&self.entries),
            stats: Arc::clone(&self.stats),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_basic_cache_operations() {
        let cache: LruCache<String, i32> = LruCache::new(CacheConfig {
            max_size: 100,
            ttl: Duration::from_secs(1),
        });

        // Test set and get (with a 1s per-entry TTL)
        cache
            .put("key1".into(), 42, Some(Duration::from_secs(1)))
            .await
            .unwrap();
        let value = cache.get(&"key1".into()).await.unwrap();
        assert_eq!(value, Some(42));

        // Test expiration
        sleep(Duration::from_secs(2)).await;
        let value = cache.get(&"key1".into()).await.unwrap();
        assert_eq!(value, None);

        // Test removal
        cache.put("key2".into(), 24, None).await.unwrap();
        cache.remove(&"key2".into()).await.unwrap();
        let value = cache.get(&"key2".into()).await.unwrap();
        assert_eq!(value, None);
    }
}

#[async_trait]
pub trait CacheLayer: Send + Sync {
    async fn get(&self, key: &str) -> DataResult<Option<String>>;
    async fn put(&self, key: &str, value: &str) -> DataResult<()>;
    async fn delete(&self, key: &str) -> DataResult<()>;
    async fn clear(&self) -> DataResult<()>;
}

pub struct NoopCache;

#[async_trait]
impl CacheLayer for NoopCache {
    async fn get(&self, _key: &str) -> DataResult<Option<String>> {
        Ok(None)
    }

    async fn put(&self, _key: &str, _value: &str) -> DataResult<()> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> DataResult<()> {
        Ok(())
    }

    async fn clear(&self) -> DataResult<()> {
        Ok(())
    }
}
