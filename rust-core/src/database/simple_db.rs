//! Simple database implementation without connection pooling for debugging

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::{DatabaseConfig, models::*};
use super::schema;

/// Simple database implementation without connection pooling
pub struct SimpleDatabase {
    conn: Arc<Mutex<Connection>>,
    config: DatabaseConfig,
}

impl SimpleDatabase {
    /// Create a new simple database instance
    pub fn new(config: DatabaseConfig) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = Path::new(&config.path).parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create database directory: {:?}", parent))?;
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE;

        let conn = Connection::open_with_flags(&config.path, flags)
            .with_context(|| format!("Failed to open database at: {}", config.path))?;

        // Minimal pragmas for testing

        // Initialize basic schema for testing - minimal version
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT,
                importance REAL NOT NULL DEFAULT 0.5,
                ttl_hours INTEGER,
                is_compressed INTEGER NOT NULL DEFAULT 0
            )",
            [],
        ).context("Failed to create memories table")?;

        let conn = Arc::new(Mutex::new(conn));

        Ok(Self { conn, config })
    }

    /// Save a memory item
    pub fn save_memory(&self, memory: &MemoryItem) -> Result<String> {
        let conn = self.conn.lock().unwrap();

        let memory_id = uuid::Uuid::new_v4().to_string();
        let metadata_json = serde_json::to_string(&memory.metadata)
            .context("Failed to serialize metadata")?;

        conn.execute(
            "INSERT INTO memories (id, user_id, session_id, content, metadata, importance, created_at, updated_at, expires_at, ttl_hours, is_compressed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            (
                &memory_id,
                &memory.user_id,
                &memory.session_id,
                &memory.content,
                &metadata_json,
                memory.importance,
                memory.created_at.to_rfc3339(),
                memory.updated_at.to_rfc3339(),
                memory.expires_at.map(|dt| dt.to_rfc3339()),
                memory.ttl_hours.map(|ttl| ttl as i64),
                memory.is_compressed as i64,
            ),
        ).context("Failed to insert memory")?;

        // FTS insert temporarily disabled for testing

        Ok(memory_id)
    }

    /// Get a memory by ID
    pub fn get_memory(&self, memory_id: &str) -> Result<Option<MemoryItem>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, user_id, session_id, content, metadata, importance, created_at, updated_at, expires_at, ttl_hours, is_compressed
             FROM memories WHERE id = ?1"
        )?;

        let memory = stmt.query_row([memory_id], |row| {
            let metadata_str: String = row.get(4)?;
            let metadata = serde_json::from_str(&metadata_str).unwrap_or_default();

            let created_at_str: String = row.get(6)?;
            let updated_at_str: String = row.get(7)?;
            let expires_at_str: Option<String> = row.get(8)?;

            Ok(MemoryItem {
                id: row.get(0)?,
                user_id: row.get(1)?,
                session_id: row.get(2)?,
                content: row.get(3)?,
                content_vector: None,
                #[cfg(feature = "vector-search")]
                embedding: None,
                #[cfg(feature = "vector-search")]
                embedding_model: None,
                metadata,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap().with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at_str)
                    .unwrap().with_timezone(&chrono::Utc),
                expires_at: expires_at_str.and_then(|s|
                    chrono::DateTime::parse_from_rfc3339(&s).ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                ),
                importance: row.get(5)?,
                ttl_hours: row.get::<_, Option<i64>>(9)?.map(|ttl| ttl as u32),
                is_compressed: row.get::<_, i64>(10)? != 0,
                compressed_from: Vec::new(),
            })
        }).optional()?;

        Ok(memory)
    }

    /// Recall memories with filters
    pub fn recall_memories(&self, filter: &QueryFilter) -> Result<PaginatedResponse<MemoryItem>> {
        let conn = self.conn.lock().unwrap();

        let mut query = String::from("SELECT id, user_id, session_id, content, metadata, importance, created_at, updated_at, expires_at, ttl_hours, is_compressed FROM memories WHERE 1=1");
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Build WHERE clause
        if let Some(user_id) = &filter.user_id {
            query.push_str(" AND user_id = ?");
            params.push(Box::new(user_id.clone()));
        }

        if let Some(session_id) = &filter.session_id {
            query.push_str(" AND session_id = ?");
            params.push(Box::new(session_id.clone()));
        }

        if let Some(date_from) = &filter.date_from {
            query.push_str(" AND created_at >= ?");
            params.push(Box::new(date_from.to_rfc3339()));
        }

        if let Some(date_to) = &filter.date_to {
            query.push_str(" AND created_at <= ?");
            params.push(Box::new(date_to.to_rfc3339()));
        }

        if let Some(min_importance) = &filter.min_importance {
            query.push_str(" AND importance >= ?");
            params.push(Box::new(*min_importance));
        }

        query.push_str(" ORDER BY created_at DESC");

        // Add limit and offset
        let limit = filter.limit.unwrap_or(50);
        let offset = filter.offset.unwrap_or(0);
        query.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

        let mut stmt = conn.prepare(&query)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let memory_iter = stmt.query_map(&param_refs[..], |row| {
            let metadata_str: String = row.get(4)?;
            let metadata = serde_json::from_str(&metadata_str).unwrap_or_default();

            let created_at_str: String = row.get(6)?;
            let updated_at_str: String = row.get(7)?;
            let expires_at_str: Option<String> = row.get(8)?;

            Ok(MemoryItem {
                id: row.get(0)?,
                user_id: row.get(1)?,
                session_id: row.get(2)?,
                content: row.get(3)?,
                content_vector: None,
                #[cfg(feature = "vector-search")]
                embedding: None,
                #[cfg(feature = "vector-search")]
                embedding_model: None,
                metadata,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap().with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at_str)
                    .unwrap().with_timezone(&chrono::Utc),
                expires_at: expires_at_str.and_then(|s|
                    chrono::DateTime::parse_from_rfc3339(&s).ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                ),
                importance: row.get(5)?,
                ttl_hours: row.get::<_, Option<i64>>(9)?.map(|ttl| ttl as u32),
                is_compressed: row.get::<_, i64>(10)? != 0,
                compressed_from: Vec::new(),
            })
        })?;

        let mut memories = Vec::new();
        for memory in memory_iter {
            memories.push(memory?);
        }

        // Get total count
        let mut count_query = String::from("SELECT COUNT(*) FROM memories WHERE 1=1");
        if let Some(user_id) = &filter.user_id {
            count_query.push_str(" AND user_id = ?");
        }
        if let Some(session_id) = &filter.session_id {
            count_query.push_str(" AND session_id = ?");
        }
        if let Some(date_from) = &filter.date_from {
            count_query.push_str(" AND created_at >= ?");
        }
        if let Some(date_to) = &filter.date_to {
            count_query.push_str(" AND created_at <= ?");
        }
        if let Some(min_importance) = &filter.min_importance {
            count_query.push_str(" AND importance >= ?");
        }

        let mut count_stmt = conn.prepare(&count_query)?;
        let total_count: i64 = count_stmt.query_row(&param_refs[..], |row| row.get(0))?;

        let page = offset / limit;
        let total_pages = (total_count as usize + limit - 1) / limit;

        Ok(PaginatedResponse {
            data: memories,
            total_count: total_count,
            page,
            per_page: limit,
            total_pages,
            has_next: page < total_pages.saturating_sub(1),
            has_prev: page > 0,
        })
    }
}