//! Database connection pool implementation for better concurrency

use anyhow::{Context, Result};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;
use std::path::Path;
use std::time::Duration;

use super::DatabaseConfig;

/// Connection pool wrapper for SQLite
pub struct ConnectionPool {
    pool: Pool<SqliteConnectionManager>,
    config: DatabaseConfig,
}

impl ConnectionPool {
    /// Create a new connection pool with the given configuration
    pub fn new(config: DatabaseConfig) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = Path::new(&config.path).parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create database directory: {:?}", parent))?;
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;

        // Clone config values to avoid move issues
        let enable_wal = config.enable_wal;
        let cache_size = config.cache_size;
        let busy_timeout = config.busy_timeout;
        let synchronous = config.synchronous.clone();

        let manager = SqliteConnectionManager::file(&config.path)
            .with_flags(flags)
            .with_init(move |conn| {
                // Apply configuration to each connection
                if enable_wal {
                    conn.execute("PRAGMA journal_mode = WAL", [])?;
                }

                conn.execute(&format!("PRAGMA cache_size = {}", cache_size), [])?;
                conn.execute(&format!("PRAGMA busy_timeout = {}", busy_timeout), [])?;
                conn.execute(&format!("PRAGMA synchronous = {}", synchronous), [])?;
                conn.execute("PRAGMA temp_store = memory", [])?;
                conn.execute("PRAGMA mmap_size = 268435456", [])?; // 256MB mmap
                conn.execute("PRAGMA foreign_keys = ON", [])?;

                Ok(())
            });

        let pool = Pool::builder()
            .max_size(config.max_connections)
            .min_idle(Some(config.min_connections))
            .connection_timeout(Duration::from_secs(30))
            .idle_timeout(Some(Duration::from_secs(600))) // 10 minutes
            .max_lifetime(Some(Duration::from_secs(3600))) // 1 hour
            .build(manager)
            .context("Failed to create connection pool")?;

        log::info!(
            "Database connection pool created: {} max connections",
            config.max_connections
        );

        Ok(Self { pool, config })
    }

    /// Get a connection from the pool
    pub fn get_connection(&self) -> Result<PooledConnection<SqliteConnectionManager>> {
        self.pool
            .get()
            .context("Failed to get connection from pool")
            .map_err(|e| {
                log::error!("Connection pool error: {}", e);
                e
            })
    }

    /// Get pool status
    pub fn status(&self) -> PoolStatus {
        let state = self.pool.state();
        PoolStatus {
            connections: state.connections,
            idle_connections: state.idle_connections,
            max_connections: self.config.max_connections,
            min_connections: self.config.min_connections,
        }
    }

    /// Execute a read-only query with automatic retry
    pub fn with_read_connection<F, R>(&self, mut f: F) -> Result<R>
    where
        F: FnMut(&rusqlite::Connection) -> Result<R>,
    {
        let mut attempts = 0;
        let max_attempts = 3;

        loop {
            attempts += 1;
            let conn = self.get_connection()?;

            match f(&*conn) {
                Ok(result) => return Ok(result),
                Err(e) if attempts < max_attempts => {
                    log::warn!(
                        "Read query failed, retrying (attempt {}/{}): {}",
                        attempts,
                        max_attempts,
                        e
                    );
                    std::thread::sleep(Duration::from_millis(100 * attempts as u64));
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Execute a write query with automatic retry and transaction
    pub fn with_write_transaction<F, R>(&self, mut f: F) -> Result<R>
    where
        F: FnMut(&rusqlite::Transaction) -> Result<R>,
    {
        let mut attempts = 0;
        let max_attempts = 3;

        loop {
            attempts += 1;
            let conn = self.get_connection()?;

            let tx_result = conn.unchecked_transaction();
            match tx_result {
                Ok(tx) => {
                    match f(&tx) {
                        Ok(result) => match tx.commit() {
                            Ok(()) => return Ok(result),
                            Err(e) if attempts < max_attempts => {
                                log::warn!("Transaction commit failed, retrying: {}", e);
                                std::thread::sleep(Duration::from_millis(100 * attempts as u64));
                                continue;
                            }
                            Err(e) => return Err(e.into()),
                        },
                        Err(e) => {
                            let _ = tx.rollback(); // Ignore rollback errors
                            if attempts < max_attempts {
                                log::warn!("Transaction failed, retrying: {}", e);
                                std::thread::sleep(Duration::from_millis(100 * attempts as u64));
                                continue;
                            } else {
                                return Err(e);
                            }
                        }
                    }
                }
                Err(e) if attempts < max_attempts => {
                    log::warn!("Failed to start transaction, retrying: {}", e);
                    std::thread::sleep(Duration::from_millis(100 * attempts as u64));
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

impl Clone for ConnectionPool {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            config: self.config.clone(),
        }
    }
}

/// Pool status information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PoolStatus {
    pub connections: u32,
    pub idle_connections: u32,
    pub max_connections: u32,
    pub min_connections: u32,
}

impl PoolStatus {
    pub fn utilization(&self) -> f32 {
        if self.max_connections == 0 {
            0.0
        } else {
            (self.connections - self.idle_connections) as f32 / self.max_connections as f32
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.connections > 0 && self.utilization() < 0.9
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config() -> (DatabaseConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = DatabaseConfig {
            path: temp_dir
                .path()
                .join("test.db")
                .to_string_lossy()
                .to_string(),
            max_connections: 5,
            min_connections: 1,
            ..Default::default()
        };
        (config, temp_dir)
    }

    #[test]
    fn test_pool_creation() {
        let (config, _temp_dir) = test_config();
        let pool = ConnectionPool::new(config).unwrap();

        let status = pool.status();
        assert!(status.max_connections > 0);
        assert!(status.is_healthy());
    }

    #[test]
    fn test_read_connection() {
        let (config, _temp_dir) = test_config();
        let pool = ConnectionPool::new(config).unwrap();

        let result = pool.with_read_connection(|conn| {
            conn.execute("CREATE TABLE test (id INTEGER)", [])
                .map_err(|e| anyhow::anyhow!(e))
        });

        assert!(result.is_ok());
    }

    #[test]
    fn test_write_transaction() {
        let (config, _temp_dir) = test_config();
        let pool = ConnectionPool::new(config).unwrap();

        // Create table first
        pool.with_write_transaction(|tx| {
            tx.execute("CREATE TABLE test (id INTEGER, value TEXT)", [])?;
            Ok(())
        })
        .unwrap();

        // Test transaction
        let result = pool.with_write_transaction(|tx| {
            tx.execute("INSERT INTO test (id, value) VALUES (1, 'test')", [])?;
            tx.execute("INSERT INTO test (id, value) VALUES (2, 'test2')", [])?;
            Ok(42)
        });

        assert_eq!(result.unwrap(), 42);

        // Verify data was committed
        let count: i64 = pool
            .with_read_connection(|conn| {
                conn.query_row("SELECT COUNT(*) FROM test", [], |row| Ok(row.get(0)?))
                    .map_err(|e| anyhow::anyhow!(e))
            })
            .unwrap();

        assert_eq!(count, 2);
    }

    #[test]
    fn test_concurrent_access() {
        let (config, _temp_dir) = test_config();
        let pool = ConnectionPool::new(config).unwrap();

        // Create table
        pool.with_write_transaction(|tx| {
            tx.execute(
                "CREATE TABLE concurrent_test (id INTEGER, thread_id INTEGER)",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        // Spawn multiple threads
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let pool = pool.clone();
                std::thread::spawn(move || {
                    pool.with_write_transaction(|tx| {
                        tx.execute(
                            "INSERT INTO concurrent_test (id, thread_id) VALUES (?1, ?2)",
                            [i, i * 100],
                        )?;
                        Ok(())
                    })
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap().unwrap();
        }

        // Verify all inserts succeeded
        let count: i64 = pool
            .with_read_connection(|conn| {
                conn.query_row("SELECT COUNT(*) FROM concurrent_test", [], |row| {
                    Ok(row.get(0)?)
                })
                .map_err(|e| anyhow::anyhow!(e))
            })
            .unwrap();

        assert_eq!(count, 10);
    }
}
