use crate::{config::CacheConfig, error::Result};
use skytable::actions::Actions;
use skytable::sync::Connection;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Cache manager for handling SkyTable operations
pub struct CacheManager {
    /// SkyTable connection
    conn: Option<Arc<RwLock<Connection>>>,
    /// Cache configuration
    config: CacheConfig,
}

impl CacheManager {
    /// Create a new cache manager
    pub async fn new(config: &CacheConfig) -> Result<Self> {
        info!("Initializing cache manager...");

        // Connect to SkyTable
        match Connection::new(&config.skytable.host, config.skytable.port) {
            Ok(conn) => {
                info!(
                    "Connected to SkyTable at {}:{}",
                    config.skytable.host, config.skytable.port
                );
                Ok(Self {
                    conn: Some(Arc::new(RwLock::new(conn))),
                    config: config.clone(),
                })
            }
            Err(e) => {
                warn!(
                    "Failed to connect to SkyTable: {}. Caching will be disabled.",
                    e
                );
                Ok(Self {
                    conn: None,
                    config: config.clone(),
                })
            }
        }
    }

    /// Set a key-value pair with optional TTL
    pub async fn set(&self, key: &str, value: &[u8], ttl: Option<u64>) -> Result<()> {
        let conn_arc = match &self.conn {
            Some(c) => c,
            None => return Ok(()),
        };

        let mut conn = conn_arc.write().await;

        // Convert bytes to string for skytable
        let value_str = String::from_utf8_lossy(value);

        if let Some(_ttl) = ttl {
            // TTL not supported in this version, just set without TTL
            conn.set(key, &*value_str)?;
        } else {
            conn.set(key, &*value_str)?;
        }

        Ok(())
    }

    /// Get a value by key
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let conn_arc = match &self.conn {
            Some(c) => c,
            None => return Ok(None),
        };

        let mut conn = conn_arc.write().await;
        let value: String = conn.get(key)?;
        Ok(Some(value.into_bytes()))
    }

    /// Delete a key
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let conn_arc = match &self.conn {
            Some(c) => c,
            None => return Ok(false),
        };

        let mut conn = conn_arc.write().await;
        let deleted = conn.del(key)?;
        Ok(deleted > 0)
    }

    /// Check if a key exists
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let conn_arc = match &self.conn {
            Some(c) => c,
            None => return Ok(false),
        };

        let mut conn = conn_arc.write().await;
        let exists = conn.exists(key)?;
        Ok(exists > 0)
    }

    /// Set multiple key-value pairs
    pub async fn set_multiple(&self, pairs: &[(&str, &[u8])]) -> Result<()> {
        let conn_arc = match &self.conn {
            Some(c) => c,
            None => return Ok(()),
        };

        let mut conn = conn_arc.write().await;
        // Convert pairs to string format
        let string_pairs: Vec<(&str, String)> = pairs
            .iter()
            .map(|(k, v)| (*k, String::from_utf8_lossy(v).to_string()))
            .collect();

        // Set each pair individually since mset has different signature
        for (key, value) in string_pairs {
            conn.set(key, &value)?;
        }
        Ok(())
    }

    /// Get multiple values by keys
    pub async fn get_multiple(&self, keys: &[&str]) -> Result<Vec<Option<Vec<u8>>>> {
        let conn_arc = match &self.conn {
            Some(c) => c,
            None => return Ok(keys.iter().map(|_| None).collect()),
        };

        let mut conn = conn_arc.write().await;
        let values: Vec<String> = conn.mget(keys)?;
        Ok(values.into_iter().map(|v| Some(v.into_bytes())).collect())
    }

    /// Delete multiple keys
    pub async fn delete_multiple(&self, keys: &[&str]) -> Result<u64> {
        let conn_arc = match &self.conn {
            Some(c) => c,
            None => return Ok(0),
        };

        let mut conn = conn_arc.write().await;
        // Delete each key individually since mdel doesn't exist
        let mut deleted = 0;
        for key in keys {
            let result = conn.del(*key)?;
            if result > 0 {
                deleted += 1;
            }
        }
        Ok(deleted)
    }

    // Note: The following methods are not available in the current skytable version
    // They are commented out to avoid compilation errors

    /*
    /// Increment a counter
    pub async fn increment(&self, key: &str) -> Result<i64> {
        let mut conn = self.conn.write().await;
        let value = conn.incr(key)?;
        Ok(value)
    }

    /// Decrement a counter
    pub async fn decrement(&self, key: &str) -> Result<i64> {
        let mut conn = self.conn.write().await;
        let value = conn.decr(key)?;
        Ok(value)
    }

    /// Add to a set
    pub async fn sadd(&self, key: &str, member: &str) -> Result<bool> {
        let mut conn = self.conn.write().await;
        let added = conn.sadd(key, member)?;
        Ok(added)
    }

    /// Remove from a set
    pub async fn srem(&self, key: &str, member: &str) -> Result<bool> {
        let mut conn = self.conn.write().await;
        let removed = conn.srem(key, member)?;
        Ok(removed)
    }

    /// Check if a member exists in a set
    pub async fn sismember(&self, key: &str, member: &str) -> Result<bool> {
        let mut conn = self.conn.write().await;
        let exists = conn.sismember(key, member)?;
        Ok(exists)
    }

    /// Get all members of a set
    pub async fn smembers(&self, key: &str) -> Result<Vec<String>> {
        let mut conn = self.conn.write().await;
        let members = conn.smembers(key)?;
        Ok(members)
    }

    /// Push to a list
    pub async fn lpush(&self, key: &str, value: &str) -> Result<u64> {
        let mut conn = self.conn.write().await;
        let len = conn.lpush(key, value)?;
        Ok(len)
    }

    /// Pop from a list
    pub async fn lpop(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.conn.write().await;
        let value = conn.lpop(key)?;
        Ok(value)
    }

    /// Get list length
    pub async fn llen(&self, key: &str) -> Result<u64> {
        let mut conn = self.conn.write().await;
        let len = conn.llen(key)?;
        Ok(len)
    }

    /// Get list range
    pub async fn lrange(&self, key: &str, start: i64, stop: i64) -> Result<Vec<String>> {
        let mut conn = self.conn.write().await;
        let values = conn.lrange(key, start, stop)?;
        Ok(values)
    }
    */

    /// Clear all keys
    pub async fn clear(&self) -> Result<()> {
        let conn_arc = match &self.conn {
            Some(c) => c,
            None => return Ok(()),
        };

        let mut conn = conn_arc.write().await;
        conn.flushdb()?;
        Ok(())
    }

    /// Get cache stats
    pub async fn stats(&self) -> Result<CacheStats> {
        // Note: info() method is not available in current skytable version
        // Return default stats for now
        Ok(CacheStats {
            keys_count: 0,
            memory_used: 0,
            hits: 0,
            misses: 0,
        })
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of keys in cache
    pub keys_count: u64,
    /// Memory used in bytes
    pub memory_used: u64,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
}
