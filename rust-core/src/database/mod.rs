//! Database module for MindCache
//!
//! Provides SQLite-based storage with FTS5 full-text search capabilities.
pub mod models;
pub mod pool;
pub mod schema;
pub mod simple_db;

#[cfg(feature = "vector-search")]
pub mod vector;

#[cfg(feature = "async")]
pub mod async_db;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::database::models::{MemoryItem, PaginatedResponse, QueryFilter};
use crate::database::pool::ConnectionPool;

/// Database configuration with connection pooling support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
    pub enable_wal: bool,
    pub cache_size: i64,
    pub busy_timeout: u32,
    pub synchronous: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub enable_read_replicas: bool,
    pub read_replica_paths: Vec<String>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "mindcache.db".to_string(),
            enable_wal: true,
            cache_size: -64000,  // 64MB cache
            busy_timeout: 30000, // 30 seconds
            synchronous: "NORMAL".to_string(),
            max_connections: 10,
            min_connections: 2,
            enable_read_replicas: false,
            read_replica_paths: Vec::new(),
        }
    }
}

/// High-performance database with connection pooling and read replicas
pub struct Database {
    write_pool: ConnectionPool,
    read_pools: Vec<ConnectionPool>,
    config: DatabaseConfig,
    read_replica_index: std::sync::atomic::AtomicUsize,
}

impl Database {
    /// Create a new database instance with connection pooling
    pub fn new(config: DatabaseConfig) -> Result<Self> {
        // Create write pool (primary database)
        let write_pool = ConnectionPool::new(config.clone())?;

        // Initialize schema on primary database
        write_pool.with_write_transaction(|tx| {
            tx.execute_batch(schema::SCHEMA_SQL)
                .context("Failed to initialize database schema")?;
            tx.execute_batch(schema::INDEXES_SQL)
                .context("Failed to create database indexes")?;
            tx.execute_batch(schema::FTS_SQL)
                .context("Failed to initialize FTS5 tables")?;
            Ok(())
        })?;

        // Create read replica pools if enabled
        let mut read_pools = Vec::new();
        if config.enable_read_replicas {
            for replica_path in &config.read_replica_paths {
                let mut replica_config = config.clone();
                replica_config.path = replica_path.clone();

                // Read replicas can have different connection settings
                replica_config.max_connections = config.max_connections / 2; // Fewer connections for replicas

                let replica_pool = ConnectionPool::new(replica_config)?;
                read_pools.push(replica_pool);

                log::info!("Initialized read replica: {}", replica_path);
            }
        }

        // If no read replicas, use write pool for reads too
        if read_pools.is_empty() {
            read_pools.push(write_pool.clone());
        }

        log::info!(
            "Database initialized successfully with {} read pools",
            read_pools.len()
        );

        Ok(Self {
            write_pool,
            read_pools,
            config,
            read_replica_index: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// Get a read pool using round-robin selection
    fn get_read_pool(&self) -> &ConnectionPool {
        if self.read_pools.len() == 1 {
            &self.read_pools[0]
        } else {
            let index = self
                .read_replica_index
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            &self.read_pools[index % self.read_pools.len()]
        }
    }

    /// Save a memory item (write operation)
    pub fn save_memory(&self, memory: &MemoryItem) -> Result<String> {
        // Validate input
        memory.validate().context("Memory validation failed")?;

        let id = if memory.id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            memory.id.clone()
        };

        let now = Utc::now();
        let expires_at = memory
            .ttl_hours
            .map(|ttl| now + chrono::Duration::hours(ttl as i64));

        self.write_pool.with_write_transaction(|tx| {
            // Insert into memories table
            tx.execute(
                r#"
                INSERT OR REPLACE INTO memories (
                    id, user_id, session_id, content, content_vector, metadata,
                    created_at, updated_at, expires_at, importance, ttl_hours,
                    is_compressed, compressed_from
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
                rusqlite::params![
                    id,
                    memory.user_id,
                    memory.session_id,
                    memory.content,
                    memory.content_vector,
                    serde_json::to_string(&memory.metadata)?,
                    memory.created_at,
                    now, // updated_at
                    expires_at,
                    memory.importance,
                    memory.ttl_hours,
                    memory.is_compressed,
                    serde_json::to_string(&memory.compressed_from)?,
                ],
            )?;

            // Insert into FTS5 table for full-text search
            tx.execute(
                "INSERT OR REPLACE INTO memories_fts (rowid, content) VALUES ((SELECT rowid FROM memories WHERE id = ?1), ?2)",
                rusqlite::params![id, memory.content],
            )?;

            // Update session last_active
            tx.execute(
                "UPDATE sessions SET last_active = ?1 WHERE id = ?2",
                rusqlite::params![now, memory.session_id],
            )?;

            Ok(())
        })?;

        log::debug!("Saved memory: {} for user: {}", id, memory.user_id);
        Ok(id)
    }

    /// Recall memories with pagination and filtering (read operation)
    pub fn recall_memories(&self, filter: &QueryFilter) -> Result<PaginatedResponse<MemoryItem>> {
        // Validate filter
        filter.validate().context("Filter validation failed")?;

        let read_pool = self.get_read_pool();

        read_pool.with_read_connection(|conn| {
            let (query, count_query, params) = self.build_recall_query(filter)?;

            // Get total count
            let total_count: i64 = {
                let mut stmt = conn.prepare(&count_query)?;
                let params_refs: Vec<&dyn rusqlite::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();
                stmt.query_row(&params_refs[..], |row| Ok(row.get(0)?))?
            };

            // Calculate pagination info
            let page = filter
                .offset
                .map(|o| o / filter.limit.unwrap_or(50))
                .unwrap_or(0);
            let per_page = filter.limit.unwrap_or(50);
            let total_pages = ((total_count as f64) / (per_page as f64)).ceil() as usize;

            // Execute main query
            let mut stmt = conn.prepare(&query)?;
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            let memory_iter = stmt.query_map(&params_refs[..], |row| {
                Ok(MemoryItem {
                    id: row.get("id")?,
                    user_id: row.get("user_id")?,
                    session_id: row.get("session_id")?,
                    content: row.get("content")?,
                    content_vector: row.get("content_vector")?,
                    #[cfg(feature = "vector-search")]
                    embedding: None,
                    #[cfg(feature = "vector-search")]
                    embedding_model: None,
                    metadata: serde_json::from_str(&row.get::<_, String>("metadata")?)
                        .unwrap_or_default(),
                    created_at: row.get("created_at")?,
                    updated_at: row.get("updated_at")?,
                    expires_at: row.get("expires_at")?,
                    importance: row.get("importance")?,
                    ttl_hours: row.get("ttl_hours")?,
                    is_compressed: row.get("is_compressed")?,
                    compressed_from: serde_json::from_str(
                        &row.get::<_, String>("compressed_from")?,
                    )
                    .unwrap_or_default(),
                })
            })?;

            let mut memories = Vec::new();
            for memory in memory_iter {
                memories.push(memory?);
            }

            Ok(PaginatedResponse {
                data: memories,
                total_count,
                page,
                per_page,
                total_pages,
                has_next: page < total_pages.saturating_sub(1),
                has_prev: page > 0,
            })
        })
    }

    /// Build SQL query for recall with filters (helper method)
    fn build_recall_query(
        &self,
        filter: &QueryFilter,
    ) -> Result<(String, String, Vec<Box<dyn rusqlite::ToSql>>)> {
        let mut conditions = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut param_index = 1;

        // Base conditions (always filter expired and deleted)
        conditions.push("(expires_at IS NULL OR expires_at > datetime('now'))".to_string());
        conditions.push("is_compressed = 0".to_string());

        // User filter
        if let Some(user_id) = &filter.user_id {
            conditions.push(format!("user_id = ?{}", param_index));
            params.push(Box::new(user_id.clone()));
            param_index += 1;
        }

        // Session filter
        if let Some(session_id) = &filter.session_id {
            conditions.push(format!("session_id = ?{}", param_index));
            params.push(Box::new(session_id.clone()));
            param_index += 1;
        }

        // Date range filters
        if let Some(date_from) = filter.date_from {
            conditions.push(format!("created_at >= ?{}", param_index));
            params.push(Box::new(date_from));
            param_index += 1;
        }

        if let Some(date_to) = filter.date_to {
            conditions.push(format!("created_at <= ?{}", param_index));
            params.push(Box::new(date_to));
            param_index += 1;
        }

        // Importance filter
        if let Some(min_importance) = filter.min_importance {
            conditions.push(format!("importance >= ?{}", param_index));
            params.push(Box::new(min_importance));
            param_index += 1;
        }

        let base_table = if let Some(keywords) = &filter.keywords {
            if !keywords.is_empty() {
                // Use FTS5 for full-text search
                let search_query = keywords.join(" OR ");
                conditions.push(format!("memories.rowid IN (SELECT rowid FROM memories_fts WHERE memories_fts MATCH ?{})", param_index));
                params.push(Box::new(search_query));
                param_index += 1;
                "memories"
            } else {
                "memories"
            }
        } else {
            "memories"
        };

        let where_clause = if conditions.is_empty() {
            "".to_string()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Main query with pagination
        let mut query = format!(
            r#"
            SELECT id, user_id, session_id, content, content_vector, metadata,
                   created_at, updated_at, expires_at, importance, ttl_hours,
                   is_compressed, compressed_from
            FROM {} {}
            ORDER BY created_at DESC, importance DESC
            "#,
            base_table, where_clause
        );

        // Add pagination
        if let Some(limit) = filter.limit {
            query.push_str(&format!(" LIMIT ?{}", param_index));
            params.push(Box::new(limit as i64));
            param_index += 1;
        }

        if let Some(offset) = filter.offset {
            query.push_str(&format!(" OFFSET ?{}", param_index));
            params.push(Box::new(offset as i64));
        }

        // Count query
        let count_query = format!("SELECT COUNT(*) FROM {} {}", base_table, where_clause);

        Ok((query, count_query, params))
    }

    /// Get a memory by ID (read operation)
    pub fn get_memory(&self, id: &str) -> Result<Option<MemoryItem>> {
        let read_pool = self.get_read_pool();

        read_pool.with_read_connection(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, user_id, session_id, content, content_vector, metadata,
                       created_at, updated_at, expires_at, importance, ttl_hours,
                       is_compressed, compressed_from
                FROM memories
                WHERE id = ?1 AND (expires_at IS NULL OR expires_at > datetime('now'))
                "#,
            )?;

            let memory = stmt
                .query_row(rusqlite::params![id], |row| {
                    Ok(MemoryItem {
                        id: row.get("id")?,
                        user_id: row.get("user_id")?,
                        session_id: row.get("session_id")?,
                        content: row.get("content")?,
                        content_vector: row.get("content_vector")?,
                        #[cfg(feature = "vector-search")]
                        embedding: None,
                        #[cfg(feature = "vector-search")]
                        embedding_model: None,
                        metadata: serde_json::from_str(&row.get::<_, String>("metadata")?)
                            .unwrap_or_default(),
                        created_at: row.get("created_at")?,
                        updated_at: row.get("updated_at")?,
                        expires_at: row.get("expires_at")?,
                        importance: row.get("importance")?,
                        ttl_hours: row.get("ttl_hours")?,
                        is_compressed: row.get("is_compressed")?,
                        compressed_from: serde_json::from_str(
                            &row.get::<_, String>("compressed_from")?,
                        )
                        .unwrap_or_default(),
                    })
                })
                .optional()?;

            Ok(memory)
        })
    }

    /// Delete a memory by ID (write operation)
    pub fn delete_memory(&self, id: &str) -> Result<bool> {
        self.write_pool.with_write_transaction(|tx| {
            // Delete from FTS table first
            tx.execute(
                "DELETE FROM memories_fts WHERE rowid = (SELECT rowid FROM memories WHERE id = ?1)",
                rusqlite::params![id],
            )?;

            // Delete from main table
            let rows_affected =
                tx.execute("DELETE FROM memories WHERE id = ?1", rusqlite::params![id])?;

            Ok(rows_affected > 0)
        })
    }

    /// Cleanup expired memories (write operation)
    pub fn cleanup_expired(&self) -> Result<usize> {
        self.write_pool.with_write_transaction(|tx| {
            // First, delete from FTS table
            tx.execute(
                "DELETE FROM memories_fts WHERE rowid IN (SELECT rowid FROM memories WHERE expires_at IS NOT NULL AND expires_at <= datetime('now'))",
                [],
            )?;

            // Then delete from main table
            let rows_affected = tx.execute(
                "DELETE FROM memories WHERE expires_at IS NOT NULL AND expires_at <= datetime('now')",
                [],
            )?;

            log::info!("Cleaned up {} expired memories", rows_affected);
            Ok(rows_affected)
        })
    }

    /// Get database statistics (read operation)
    pub fn get_stats(&self) -> Result<serde_json::Value> {
        let read_pool = self.get_read_pool();

        read_pool.with_read_connection(|conn| {
            // Total memories
            let total_memories: i64 = conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE (expires_at IS NULL OR expires_at > datetime('now'))",
                [],
                |row| Ok(row.get(0)?)
            )?;

            // Memory by user
            let mut stmt = conn.prepare("SELECT user_id, COUNT(*) FROM memories WHERE (expires_at IS NULL OR expires_at > datetime('now')) GROUP BY user_id")?;
            let user_counts = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;

            let mut user_map = serde_json::Map::new();
            for result in user_counts {
                let (user_id, count) = result?;
                user_map.insert(user_id, serde_json::Value::Number(count.into()));
            }

            // Database file size
            let file_size = std::fs::metadata(&self.config.path)
                .map(|m| m.len())
                .unwrap_or(0);

            // Pool status
            let write_pool_status = self.write_pool.status();
            let read_pool_statuses: Vec<_> = self.read_pools.iter().map(|p| p.status()).collect();

            let stats = serde_json::json!({
                "total_memories": total_memories,
                "user_counts": user_map,
                "database_size_bytes": file_size,
                "database_path": self.config.path,
                "connection_pools": {
                    "write_pool": {
                        "connections": write_pool_status.connections,
                        "idle_connections": write_pool_status.idle_connections,
                        "max_connections": write_pool_status.max_connections,
                        "utilization": write_pool_status.utilization(),
                        "healthy": write_pool_status.is_healthy()
                    },
                    "read_pools": read_pool_statuses.iter().enumerate().map(|(i, status)| {
                        serde_json::json!({
                            "index": i,
                            "connections": status.connections,
                            "idle_connections": status.idle_connections,
                            "max_connections": status.max_connections,
                            "utilization": status.utilization(),
                            "healthy": status.is_healthy()
                        })
                    }).collect::<Vec<_>>()
                }
            });

            Ok(stats)
        })
    }

    /// Create a new session (write operation)
    pub fn create_session(&self, user_id: &str, session_name: Option<String>) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        self.write_pool.with_write_transaction(|tx| {
            tx.execute(
                r#"
                INSERT INTO sessions (id, user_id, name, created_at, last_active)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                rusqlite::params![session_id, user_id, session_name, now, now],
            )?;
            Ok(())
        })?;

        log::debug!("Created session: {} for user: {}", session_id, user_id);
        Ok(session_id)
    }

    /// Get sessions for a user with pagination (read operation)
    pub fn get_user_sessions(
        &self,
        user_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<PaginatedResponse<models::Session>> {
        let read_pool = self.get_read_pool();

        read_pool.with_read_connection(|conn| {
            // Get total count
            let total_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sessions WHERE user_id = ?1",
                rusqlite::params![user_id],
                |row| Ok(row.get(0)?),
            )?;

            // Calculate pagination
            let per_page = limit.unwrap_or(50);
            let page = offset.map(|o| o / per_page).unwrap_or(0);
            let total_pages = ((total_count as f64) / (per_page as f64)).ceil() as usize;

            // Get sessions with memory counts
            let mut stmt = conn.prepare(
                r#"
                SELECT s.id, s.user_id, s.name, s.created_at, s.last_active,
                       COALESCE(m.memory_count, 0) as memory_count
                FROM sessions s
                LEFT JOIN (
                    SELECT session_id, COUNT(*) as memory_count 
                    FROM memories 
                    WHERE expires_at IS NULL OR expires_at > datetime('now')
                    GROUP BY session_id
                ) m ON s.id = m.session_id
                WHERE s.user_id = ?1
                ORDER BY s.last_active DESC
                LIMIT ?2 OFFSET ?3
                "#,
            )?;

            let session_iter = stmt.query_map(
                rusqlite::params![user_id, per_page, offset.unwrap_or(0)],
                |row| {
                    Ok(models::Session {
                        id: row.get("id")?,
                        user_id: row.get("user_id")?,
                        name: row.get("name")?,
                        created_at: row.get("created_at")?,
                        last_active: row.get("last_active")?,
                        memory_count: row.get("memory_count")?,
                        tags: Vec::new(), // TODO: Implement tags
                        metadata: std::collections::HashMap::new(), // TODO: Implement metadata
                    })
                },
            )?;

            let mut sessions = Vec::new();
            for session in session_iter {
                sessions.push(session?);
            }

            Ok(PaginatedResponse {
                data: sessions,
                total_count,
                page,
                per_page,
                total_pages,
                has_next: page < total_pages.saturating_sub(1),
                has_prev: page > 0,
            })
        })
    }

    /// Get connection pool status for monitoring
    pub fn get_pool_status(&self) -> DatabasePoolStatus {
        DatabasePoolStatus {
            write_pool: self.write_pool.status(),
            read_pools: self.read_pools.iter().map(|p| p.status()).collect(),
        }
    }

    /// Get connection pool for vector engine initialization
    pub fn get_connection_pool(&self) -> ConnectionPool {
        self.write_pool.clone()
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            write_pool: self.write_pool.clone(),
            read_pools: self.read_pools.clone(),
            config: self.config.clone(),
            read_replica_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

/// Database pool status for monitoring
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatabasePoolStatus {
    pub write_pool: pool::PoolStatus,
    pub read_pools: Vec<pool::PoolStatus>,
}

impl DatabasePoolStatus {
    pub fn is_healthy(&self) -> bool {
        self.write_pool.is_healthy() && self.read_pools.iter().all(|p| p.is_healthy())
    }

    pub fn overall_utilization(&self) -> f32 {
        let total_pools = 1 + self.read_pools.len();
        let total_utilization = self.write_pool.utilization()
            + self.read_pools.iter().map(|p| p.utilization()).sum::<f32>();

        total_utilization / total_pools as f32
    }
}
