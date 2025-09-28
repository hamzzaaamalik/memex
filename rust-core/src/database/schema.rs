//! Database schema definitions for MindCache SQLite backend

/// Main database schema SQL
pub const SCHEMA_SQL: &str = r#"
-- Users table
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata TEXT DEFAULT '{}' -- JSON metadata
);

-- Sessions table
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_active TEXT NOT NULL DEFAULT (datetime('now')),
    tags TEXT DEFAULT '[]', -- JSON array
    metadata TEXT DEFAULT '{}', -- JSON metadata
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- Main memories table
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    content TEXT NOT NULL,
    content_vector TEXT, -- For future vector search support
    metadata TEXT DEFAULT '{}', -- JSON metadata
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT, -- NULL means no expiration
    importance REAL NOT NULL DEFAULT 0.5 CHECK (importance >= 0.0 AND importance <= 1.0),
    ttl_hours INTEGER, -- Time to live in hours
    is_compressed INTEGER NOT NULL DEFAULT 0, -- Boolean flag
    compressed_from TEXT DEFAULT '[]', -- JSON array of original memory IDs
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES sessions (id) ON DELETE CASCADE
);

-- Compressed memories table (for storing compression metadata)
CREATE TABLE IF NOT EXISTS compressed_memories (
    id TEXT PRIMARY KEY,
    original_ids TEXT NOT NULL, -- JSON array of original memory IDs
    user_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    summary TEXT NOT NULL,
    key_points TEXT DEFAULT '[]', -- JSON array
    date_range_start TEXT NOT NULL,
    date_range_end TEXT NOT NULL,
    original_count INTEGER NOT NULL,
    combined_importance REAL NOT NULL,
    compressed_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES sessions (id) ON DELETE CASCADE
);

-- Session summaries table
CREATE TABLE IF NOT EXISTS session_summaries (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    summary_text TEXT NOT NULL,
    key_topics TEXT DEFAULT '[]', -- JSON array
    memory_count INTEGER NOT NULL DEFAULT 0,
    date_range_start TEXT NOT NULL,
    date_range_end TEXT NOT NULL,
    importance_score REAL NOT NULL DEFAULT 0.5,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES sessions (id) ON DELETE CASCADE
);

-- Decay statistics table
CREATE TABLE IF NOT EXISTS decay_runs (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    memories_expired INTEGER DEFAULT 0,
    memories_compressed INTEGER DEFAULT 0,
    sessions_summarized INTEGER DEFAULT 0,
    total_memories_before INTEGER DEFAULT 0,
    total_memories_after INTEGER DEFAULT 0,
    storage_saved_bytes INTEGER DEFAULT 0,
    error_message TEXT,
    status TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'completed', 'failed'))
);

-- Configuration table for storing system settings
CREATE TABLE IF NOT EXISTS system_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// Database indexes for performance optimization
pub const INDEXES_SQL: &str = r#"
-- Indexes for memories table
CREATE INDEX IF NOT EXISTS idx_memories_user_id ON memories (user_id);
CREATE INDEX IF NOT EXISTS idx_memories_session_id ON memories (session_id);
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories (created_at);
CREATE INDEX IF NOT EXISTS idx_memories_expires_at ON memories (expires_at);
CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories (importance);
CREATE INDEX IF NOT EXISTS idx_memories_user_created ON memories (user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memories_session_created ON memories (session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memories_user_importance ON memories (user_id, importance DESC);
CREATE INDEX IF NOT EXISTS idx_memories_active ON memories (expires_at) WHERE expires_at IS NULL OR expires_at > datetime('now');

-- Indexes for sessions table
CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_last_active ON sessions (last_active DESC);
CREATE INDEX IF NOT EXISTS idx_sessions_user_active ON sessions (user_id, last_active DESC);

-- Indexes for compressed_memories table
CREATE INDEX IF NOT EXISTS idx_compressed_user_id ON compressed_memories (user_id);
CREATE INDEX IF NOT EXISTS idx_compressed_session_id ON compressed_memories (session_id);
CREATE INDEX IF NOT EXISTS idx_compressed_at ON compressed_memories (compressed_at);

-- Indexes for session_summaries table
CREATE INDEX IF NOT EXISTS idx_summaries_user_id ON session_summaries (user_id);
CREATE INDEX IF NOT EXISTS idx_summaries_created ON session_summaries (created_at DESC);

-- Indexes for decay_runs table
CREATE INDEX IF NOT EXISTS idx_decay_runs_started ON decay_runs (started_at DESC);
CREATE INDEX IF NOT EXISTS idx_decay_runs_status ON decay_runs (status);
"#;

/// FTS5 full-text search setup
pub const FTS_SQL: &str = r#"
-- Create FTS5 virtual table for full-text search on memory content
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    content,
    content='memories',
    content_rowid='rowid'
);

-- Triggers to keep FTS5 table in sync with memories table
CREATE TRIGGER IF NOT EXISTS memories_fts_insert AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS memories_fts_delete AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;

CREATE TRIGGER IF NOT EXISTS memories_fts_update AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
END;

-- Create additional FTS5 table for session summaries (for future use)
CREATE VIRTUAL TABLE IF NOT EXISTS summaries_fts USING fts5(
    summary_text,
    key_topics,
    content='session_summaries',
    content_rowid='rowid'
);

-- Triggers for summaries FTS
CREATE TRIGGER IF NOT EXISTS summaries_fts_insert AFTER INSERT ON session_summaries BEGIN
    INSERT INTO summaries_fts(rowid, summary_text, key_topics) 
    VALUES (new.rowid, new.summary_text, new.key_topics);
END;

CREATE TRIGGER IF NOT EXISTS summaries_fts_delete AFTER DELETE ON session_summaries BEGIN
    INSERT INTO summaries_fts(summaries_fts, rowid, summary_text, key_topics) 
    VALUES ('delete', old.rowid, old.summary_text, old.key_topics);
END;

CREATE TRIGGER IF NOT EXISTS summaries_fts_update AFTER UPDATE ON session_summaries BEGIN
    INSERT INTO summaries_fts(summaries_fts, rowid, summary_text, key_topics) 
    VALUES ('delete', old.rowid, old.summary_text, old.key_topics);
    INSERT INTO summaries_fts(rowid, summary_text, key_topics) 
    VALUES (new.rowid, new.summary_text, new.key_topics);
END;
"#;

/// Migration utilities
pub struct Migration {
    pub version: u32,
    pub description: String,
    pub up_sql: String,
    pub down_sql: String,
}

/// Get all available migrations
pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "Initial schema".to_string(),
            up_sql: format!("{}\n{}\n{}", SCHEMA_SQL, INDEXES_SQL, FTS_SQL),
            down_sql: r#"
                DROP TABLE IF EXISTS decay_runs;
                DROP TABLE IF EXISTS system_config;
                DROP TABLE IF EXISTS session_summaries;
                DROP TABLE IF EXISTS compressed_memories;
                DROP TRIGGER IF EXISTS summaries_fts_update;
                DROP TRIGGER IF EXISTS summaries_fts_delete;
                DROP TRIGGER IF EXISTS summaries_fts_insert;
                DROP TABLE IF EXISTS summaries_fts;
                DROP TRIGGER IF EXISTS memories_fts_update;
                DROP TRIGGER IF EXISTS memories_fts_delete;
                DROP TRIGGER IF EXISTS memories_fts_insert;
                DROP TABLE IF EXISTS memories_fts;
                DROP TABLE IF EXISTS memories;
                DROP TABLE IF EXISTS sessions;
                DROP TABLE IF EXISTS users;
            "#.to_string(),
        },
        // Future migrations can be added here
    ]
}

/// Schema version management
pub const SCHEMA_VERSION_KEY: &str = "schema_version";

/// Get current schema version from database
pub fn get_schema_version(conn: &rusqlite::Connection) -> rusqlite::Result<u32> {
    // First, ensure system_config table exists
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS system_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        "#,
        [],
    )?;
    
    match conn.query_row(
        "SELECT value FROM system_config WHERE key = ?1",
        [SCHEMA_VERSION_KEY],
        |row| {
            let version_str: String = row.get(0)?;
            Ok(version_str.parse::<u32>().unwrap_or(0))
        },
    ) {
        Ok(version) => Ok(version),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(e) => Err(e),
    }
}

/// Set schema version in database
pub fn set_schema_version(conn: &rusqlite::Connection, version: u32) -> rusqlite::Result<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO system_config (key, value, updated_at)
        VALUES (?1, ?2, datetime('now'))
        "#,
        rusqlite::params![SCHEMA_VERSION_KEY, version.to_string()],
    )?;
    Ok(())
}

/// Run database migrations
pub fn run_migrations(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let current_version = get_schema_version(conn)?;
    let migrations = get_migrations();
    let latest_version = migrations.iter().map(|m| m.version).max().unwrap_or(0);
    
    if current_version >= latest_version {
        log::info!("Database schema is up to date (version {})", current_version);
        return Ok(());
    }
    
    log::info!("Migrating database from version {} to {}", current_version, latest_version);
    
    // Run migrations in order
    for migration in migrations {
        if migration.version > current_version {
            log::info!("Running migration {}: {}", migration.version, migration.description);
            
            let tx = conn.unchecked_transaction()?;
            
            // Execute migration SQL
            tx.execute_batch(&migration.up_sql)?;
            
            // Update schema version
            set_schema_version(&tx, migration.version)?;
            
            tx.commit()?;
            
            log::info!("Migration {} completed successfully", migration.version);
        }
    }
    
    log::info!("All migrations completed successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    
    #[test]
    fn test_schema_creation() {
        let conn = Connection::open_in_memory().unwrap();
        
        // Execute schema
        conn.execute_batch(SCHEMA_SQL).unwrap();
        conn.execute_batch(INDEXES_SQL).unwrap();
        conn.execute_batch(FTS_SQL).unwrap();
        
        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| Ok(row.get::<_, String>(0)?))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        
        let expected_tables = vec![
            "compressed_memories",
            "decay_runs", 
            "memories",
            "memories_fts",
            "session_summaries",
            "sessions",
            "summaries_fts",
            "system_config",
            "users",
        ];
        
        for table in expected_tables {
            assert!(tables.contains(&table.to_string()), "Missing table: {}", table);
        }
    }
    
    #[test]
    fn test_fts5_functionality() {
        let conn = Connection::open_in_memory().unwrap();
        
        // Create schema
        conn.execute_batch(SCHEMA_SQL).unwrap();
        conn.execute_batch(FTS_SQL).unwrap();
        
        // Insert test data
        conn.execute(
            "INSERT INTO memories (id, user_id, session_id, content) VALUES ('1', 'user1', 'session1', 'This is about trading stocks')",
            [],
        ).unwrap();
        
        conn.execute(
            "INSERT INTO memories (id, user_id, session_id, content) VALUES ('2', 'user1', 'session1', 'This is about crypto trading')",
            [],
        ).unwrap();
        
        // Test FTS search
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'trading'",
            [],
            |row| Ok(row.get(0)?),
        ).unwrap();
        
        assert_eq!(count, 2);
        
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'stocks'",
            [],
            |row| Ok(row.get(0)?),
        ).unwrap();
        
        assert_eq!(count, 1);
    }
    
    #[test]
    fn test_migration_system() {
        let conn = Connection::open_in_memory().unwrap();
        
        // Initial version should be 0
        assert_eq!(get_schema_version(&conn).unwrap(), 0);
        
        // Run migrations
        run_migrations(&conn).unwrap();
        
        // Should be at latest version
        let migrations = get_migrations();
        let latest_version = migrations.iter().map(|m| m.version).max().unwrap_or(0);
        assert_eq!(get_schema_version(&conn).unwrap(), latest_version);
        
        // Running again should be no-op
        run_migrations(&conn).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), latest_version);
    }
}