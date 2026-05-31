use async_trait::async_trait;
use metrics::{counter, gauge};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, Semaphore};

use crate::error::{DataError, DataResult, PoolError};
use crate::types::PoolConfig;

/// Generic connection type that can be pooled
#[async_trait]
pub trait PoolableConnection: Send + Sync + 'static {
    /// Connect to the connection
    async fn connect(url: &str) -> DataResult<Self>
    where
        Self: Sized;

    /// Close the connection
    async fn close(&mut self) -> DataResult<()>;

    /// Check if the connection is valid
    fn is_valid(&self) -> bool;

    /// Check the health of the connection (optional async health check)
    async fn check_health(&self) -> DataResult<()>;

    /// Reset the connection state
    async fn reset(&mut self) -> DataResult<()>;

    /// Lightweight health-check that should be fast and non-blocking.
    /// Implementations can simply return `true` if the connection appears
    /// usable without performing any expensive I/O.
    fn ping(&self) -> bool {
        // Default implementation assumes the connection is valid.
        true
    }
}

/// A pooled connection wrapper
pub struct PooledConnection<'a, T: PoolableConnection> {
    pool: &'a ConnectionPool<T>,
    connection: Option<T>,
    _permit: tokio::sync::SemaphorePermit<'a>,
}

impl<'a, T: PoolableConnection> Deref for PooledConnection<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.connection.as_ref().unwrap()
    }
}

impl<'a, T: PoolableConnection> DerefMut for PooledConnection<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.connection.as_mut().unwrap()
    }
}

/// Pool statistics
#[derive(Debug, Default, Clone)]
pub struct PoolStats {
    pub acquired_total: u64,
    pub returned_total: u64,
    pub acquired_current: u64,
    pub idle_current: u64,
    pub errors_total: u64,
}

/// Connection pool implementation with monitoring
pub struct ConnectionPool<T: PoolableConnection> {
    /// Available connections
    connections: Arc<RwLock<Vec<T>>>,

    /// Pool configuration
    config: PoolConfig,

    /// Semaphore for connection count limiting
    semaphore: Arc<Semaphore>,

    /// Pool statistics
    stats: Arc<RwLock<PoolStats>>,

    url: String,
}

impl<T: PoolableConnection> ConnectionPool<T> {
    /// Create a new connection pool
    pub fn new(initial_connections: Vec<T>, max_size: usize, url: String) -> Self {
        let config = PoolConfig {
            min_size: 1,
            max_size,
            timeout_seconds: 30,
            min_idle: 1,
            max_lifetime: Some(Duration::from_secs(3600)),
            idle_timeout: Some(Duration::from_secs(300)),
            connection_timeout: Duration::from_secs(30),
        };

        Self {
            connections: Arc::new(RwLock::new(initial_connections)),
            config,
            semaphore: Arc::new(Semaphore::new(max_size)),
            stats: Arc::new(RwLock::new(PoolStats::default())),
            url,
        }
    }

    /// Get a connection from the pool
    pub async fn acquire(&self) -> DataResult<PooledConnection<'_, T>> {
        let permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| PoolError::NoAvailableConnections)?;

        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.pop() {
            let mut stats = self.stats.write().await;
            stats.acquired_current += 1;
            gauge!("pool.connections.active", stats.acquired_current as f64);
            gauge!("pool.connections.idle", stats.idle_current as f64);

            Ok(PooledConnection {
                pool: self,
                connection: Some(conn),
                _permit: permit,
            })
        } else {
            let conn = T::connect(&self.url).await?;
            let mut stats = self.stats.write().await;
            stats.acquired_current += 1;
            gauge!("pool.connections.active", stats.acquired_current as f64);
            gauge!("pool.connections.idle", stats.idle_current as f64);

            Ok(PooledConnection {
                pool: self,
                connection: Some(conn),
                _permit: permit,
            })
        }
    }

    /// Return a connection to the pool
    pub async fn release(&self, mut conn: T) -> DataResult<()> {
        if !conn.is_valid() {
            conn.reset().await?;
        }
        let mut connections = self.connections.write().await;

        // Check if we should close this connection
        if connections.len() >= self.config.max_size {
            return Ok(());
        }

        connections.push(conn);
        let mut stats = self.stats.write().await;
        stats.returned_total += 1;
        stats.acquired_current -= 1;
        stats.idle_current += 1;

        counter!("pool.connections.returned.total", 1);
        gauge!("pool.connections.active", stats.acquired_current as f64);
        gauge!("pool.connections.idle", stats.idle_current as f64);

        Ok(())
    }

    /// Get current pool statistics
    pub async fn stats(&self) -> PoolStats {
        self.stats.read().await.clone()
    }
}

impl<T: PoolableConnection> Clone for ConnectionPool<T> {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            connections: self.connections.clone(),
            config: self.config.clone(),
            semaphore: self.semaphore.clone(),
            stats: self.stats.clone(),
        }
    }
}

impl<'a, T: PoolableConnection> PooledConnection<'a, T> {
    pub fn get_mut(&mut self) -> DataResult<&mut T> {
        self.connection.as_mut().ok_or_else(|| {
            DataError::Pool(PoolError::InvalidConnection(
                "Connection is not available".to_string(),
            ))
        })
    }
}

impl<'a, T: PoolableConnection> Drop for PooledConnection<'a, T> {
    fn drop(&mut self) {
        if let Some(conn) = self.connection.take() {
            let pool = self.pool.clone();
            tokio::spawn(async move {
                if let Err(e) = pool.release(conn).await {
                    eprintln!("Failed to release connection: {}", e);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::time::sleep;

    struct MockConnection {
        id: u64,
        is_healthy: bool,
    }

    #[async_trait]
    impl PoolableConnection for MockConnection {
        async fn connect(url: &str) -> DataResult<Self>
        where
            Self: Sized,
        {
            Ok(MockConnection {
                id: url.parse::<u64>()?,
                is_healthy: true,
            })
        }

        async fn close(&mut self) -> DataResult<()> {
            Ok(())
        }

        fn is_valid(&self) -> bool {
            self.is_healthy
        }

        async fn check_health(&self) -> DataResult<()> {
            Ok(())
        }

        async fn reset(&mut self) -> DataResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_pool_basic_operations() {
        let counter = Arc::new(AtomicU64::new(0));
        let counter_clone = Arc::clone(&counter);

        let pool = ConnectionPool::new(vec![], 5, String::new());

        // Initialize pool
        pool.initialize().await.unwrap();
        assert_eq!(pool.stats().idle_current, 0);

        // Acquire connections
        let conn1 = pool.acquire().await.unwrap();
        let conn2 = pool.acquire().await.unwrap();
        assert_eq!(pool.stats().acquired_current, 2);

        // Return connections
        drop(conn1);
        drop(conn2);

        // Allow async operations to complete
        sleep(Duration::from_millis(100)).await;

        assert_eq!(pool.stats().acquired_current, 0);
        assert_eq!(pool.stats().idle_current, 5);
    }
}
