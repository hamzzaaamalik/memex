//! Vector search implementation for semantic memory retrieval

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::models::MemoryItem;
use super::ConnectionPool;

/// Vector embedding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfig {
    pub dimension: usize,
    pub similarity_threshold: f32,
    pub max_results: usize,
    pub enable_approximate_search: bool,
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            dimension: 384, // Common embedding dimension
            similarity_threshold: 0.7,
            max_results: 50,
            enable_approximate_search: true,
        }
    }
}

/// Vector search engine for semantic memory retrieval
pub struct VectorSearchEngine {
    pool: ConnectionPool,
    config: VectorConfig,
}

impl VectorSearchEngine {
    pub fn new(pool: ConnectionPool, config: VectorConfig) -> Self {
        Self { pool, config }
    }

    /// Initialize vector search tables and indexes
    pub fn initialize_schema(&self) -> Result<()> {
        self.pool.with_write_transaction(|tx| {
            // Create vector embeddings table
            tx.execute(
                r#"
                CREATE TABLE IF NOT EXISTS memory_embeddings (
                    memory_id TEXT PRIMARY KEY,
                    embedding BLOB NOT NULL,
                    model_name TEXT NOT NULL,
                    dimension INTEGER NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    FOREIGN KEY (memory_id) REFERENCES memories (id) ON DELETE CASCADE
                )
                "#,
                [],
            )?;

            // Create index for faster lookups
            tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_embeddings_model ON memory_embeddings (model_name)",
                [],
            )?;

            // Create vector similarity function (using SQLite extension or custom implementation)
            // Note: In production, you might want to use a specialized vector database like Qdrant or Weaviate
            tx.create_scalar_function(
                "cosine_similarity",
                2,
                rusqlite::functions::FunctionFlags::SQLITE_UTF8
                    | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
                move |ctx| {
                    let blob1 = ctx.get::<Vec<u8>>(0)?;
                    let blob2 = ctx.get::<Vec<u8>>(1)?;

                    let vec1 = deserialize_vector(&blob1).map_err(|_| {
                        rusqlite::Error::UserFunctionError("Invalid vector 1".into())
                    })?;
                    let vec2 = deserialize_vector(&blob2).map_err(|_| {
                        rusqlite::Error::UserFunctionError("Invalid vector 2".into())
                    })?;

                    let similarity = cosine_similarity(&vec1, &vec2);
                    Ok(similarity)
                },
            )?;

            Ok(())
        })
    }

    /// Store embedding for a memory
    pub fn store_embedding(
        &self,
        memory_id: &str,
        embedding: &[f32],
        model_name: &str,
    ) -> Result<()> {
        if embedding.len() != self.config.dimension {
            return Err(anyhow::anyhow!(
                "Embedding dimension {} doesn't match configured dimension {}",
                embedding.len(),
                self.config.dimension
            ));
        }

        let embedding_blob = serialize_vector(embedding)?;

        self.pool.with_write_transaction(|tx| {
            tx.execute(
                r#"
                INSERT OR REPLACE INTO memory_embeddings 
                (memory_id, embedding, model_name, dimension, created_at)
                VALUES (?1, ?2, ?3, ?4, datetime('now'))
                "#,
                rusqlite::params![memory_id, embedding_blob, model_name, self.config.dimension],
            )?;
            Ok(())
        })
    }

    /// Search for similar memories using vector similarity
    pub fn search_similar(
        &self,
        query_embedding: &[f32],
        model_name: &str,
        limit: Option<usize>,
    ) -> Result<Vec<VectorSearchResult>> {
        if query_embedding.len() != self.config.dimension {
            return Err(anyhow::anyhow!(
                "Query embedding dimension {} doesn't match configured dimension {}",
                query_embedding.len(),
                self.config.dimension
            ));
        }

        let query_blob = serialize_vector(query_embedding)?;
        let limit = limit
            .unwrap_or(self.config.max_results)
            .min(self.config.max_results);

        self.pool.with_read_connection(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT 
                    m.id, m.user_id, m.session_id, m.content, m.importance,
                    m.created_at, m.updated_at,
                    cosine_similarity(e.embedding, ?1) as similarity
                FROM memories m
                INNER JOIN memory_embeddings e ON m.id = e.memory_id
                WHERE e.model_name = ?2 
                    AND (m.expires_at IS NULL OR m.expires_at > datetime('now'))
                    AND cosine_similarity(e.embedding, ?1) >= ?3
                ORDER BY similarity DESC
                LIMIT ?4
                "#,
            )?;

            let results = stmt.query_map(
                rusqlite::params![
                    query_blob,
                    model_name,
                    self.config.similarity_threshold,
                    limit
                ],
                |row| {
                    Ok(VectorSearchResult {
                        memory_id: row.get("id")?,
                        user_id: row.get("user_id")?,
                        session_id: row.get("session_id")?,
                        content: row.get("content")?,
                        importance: row.get("importance")?,
                        similarity: row.get("similarity")?,
                        created_at: row.get("created_at")?,
                    })
                },
            )?;

            let mut search_results = Vec::new();
            for result in results {
                search_results.push(result?);
            }

            Ok(search_results)
        })
    }

    /// Hybrid search combining text and vector search
    pub fn hybrid_search(
        &self,
        text_query: &str,
        vector_query: &[f32],
        model_name: &str,
        text_weight: f32,
        vector_weight: f32,
        limit: Option<usize>,
    ) -> Result<Vec<HybridSearchResult>> {
        let limit = limit
            .unwrap_or(self.config.max_results)
            .min(self.config.max_results);
        let query_blob = serialize_vector(vector_query)?;

        self.pool.with_read_connection(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT 
                    m.id, m.user_id, m.session_id, m.content, m.importance,
                    m.created_at, m.updated_at,
                    cosine_similarity(e.embedding, ?1) as vector_similarity,
                    CASE 
                        WHEN fts.content IS NOT NULL THEN 1.0 
                        ELSE 0.0 
                    END as text_match,
                    (?2 * CASE WHEN fts.content IS NOT NULL THEN 1.0 ELSE 0.0 END + 
                     ?3 * cosine_similarity(e.embedding, ?1)) as combined_score
                FROM memories m
                INNER JOIN memory_embeddings e ON m.id = e.memory_id
                LEFT JOIN memories_fts fts ON m.rowid = fts.rowid AND fts MATCH ?4
                WHERE e.model_name = ?5 
                    AND (m.expires_at IS NULL OR m.expires_at > datetime('now'))
                    AND (?2 * CASE WHEN fts.content IS NOT NULL THEN 1.0 ELSE 0.0 END + 
                         ?3 * cosine_similarity(e.embedding, ?1)) >= ?6
                ORDER BY combined_score DESC
                LIMIT ?7
                "#,
            )?;

            let min_combined_score =
                text_weight * 0.5 + vector_weight * self.config.similarity_threshold;

            let results = stmt.query_map(
                rusqlite::params![
                    query_blob,
                    text_weight,
                    vector_weight,
                    text_query,
                    model_name,
                    min_combined_score,
                    limit
                ],
                |row| {
                    Ok(HybridSearchResult {
                        memory_id: row.get("id")?,
                        user_id: row.get("user_id")?,
                        session_id: row.get("session_id")?,
                        content: row.get("content")?,
                        importance: row.get("importance")?,
                        vector_similarity: row.get("vector_similarity")?,
                        text_match: row.get("text_match")?,
                        combined_score: row.get("combined_score")?,
                        created_at: row.get("created_at")?,
                    })
                },
            )?;

            let mut search_results = Vec::new();
            for result in results {
                search_results.push(result?);
            }

            Ok(search_results)
        })
    }

    /// Get embedding for a memory if it exists
    pub fn get_embedding(&self, memory_id: &str, model_name: &str) -> Result<Option<Vec<f32>>> {
        self.pool.with_read_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT embedding FROM memory_embeddings WHERE memory_id = ?1 AND model_name = ?2",
            )?;

            let embedding_blob: Option<Vec<u8>> = stmt
                .query_row(rusqlite::params![memory_id, model_name], |row| {
                    Ok(row.get("embedding")?)
                })
                .optional()?;

            match embedding_blob {
                Some(blob) => {
                    let embedding = deserialize_vector(&blob)?;
                    Ok(Some(embedding))
                }
                None => Ok(None),
            }
        })
    }

    /// Delete embedding for a memory
    pub fn delete_embedding(&self, memory_id: &str) -> Result<bool> {
        self.pool.with_write_transaction(|tx| {
            let rows_affected = tx.execute(
                "DELETE FROM memory_embeddings WHERE memory_id = ?1",
                rusqlite::params![memory_id],
            )?;
            Ok(rows_affected > 0)
        })
    }

    /// Get vector search statistics
    pub fn get_vector_stats(&self) -> Result<VectorStats> {
        self.pool.with_read_connection(|conn| {
            let total_embeddings: i64 =
                conn.query_row("SELECT COUNT(*) FROM memory_embeddings", [], |row| {
                    Ok(row.get(0)?)
                })?;

            let mut stmt = conn.prepare(
                "SELECT model_name, COUNT(*) FROM memory_embeddings GROUP BY model_name",
            )?;

            let model_counts = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;

            let mut models = HashMap::new();
            for result in model_counts {
                let (model, count) = result?;
                models.insert(model, count);
            }

            Ok(VectorStats {
                total_embeddings,
                models,
                dimension: self.config.dimension,
            })
        })
    }
}

/// Vector search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub memory_id: String,
    pub user_id: String,
    pub session_id: String,
    pub content: String,
    pub importance: f32,
    pub similarity: f32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Hybrid search result combining text and vector search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    pub memory_id: String,
    pub user_id: String,
    pub session_id: String,
    pub content: String,
    pub importance: f32,
    pub vector_similarity: f32,
    pub text_match: f32,
    pub combined_score: f32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Vector search statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStats {
    pub total_embeddings: i64,
    pub models: HashMap<String, i64>,
    pub dimension: usize,
}

/// Serialize vector to binary format for database storage
fn serialize_vector(vector: &[f32]) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for &value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}

/// Deserialize vector from binary format
fn deserialize_vector(bytes: &[u8]) -> Result<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return Err(anyhow::anyhow!("Invalid vector byte length"));
    }

    let mut vector = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        vector.push(value);
    }
    Ok(vector)
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_vector_engine() -> (VectorSearchEngine, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = super::super::DatabaseConfig {
            path: temp_dir
                .path()
                .join("test.db")
                .to_string_lossy()
                .to_string(),
            max_connections: 2,
            min_connections: 1,
            ..Default::default()
        };

        let pool = ConnectionPool::new(config).unwrap();
        let vector_config = VectorConfig {
            dimension: 4, // Small dimension for testing
            similarity_threshold: 0.5,
            max_results: 10,
            enable_approximate_search: false,
        };

        let engine = VectorSearchEngine::new(pool, vector_config);
        engine.initialize_schema().unwrap();

        (engine, temp_dir)
    }

    #[test]
    fn test_vector_serialization() {
        let vector = vec![1.0, -0.5, 0.25, 0.0];
        let bytes = serialize_vector(&vector).unwrap();
        let deserialized = deserialize_vector(&bytes).unwrap();

        assert_eq!(vector, deserialized);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let c = vec![1.0, 0.0, 0.0];

        assert_eq!(cosine_similarity(&a, &b), 0.0); // Orthogonal vectors
        assert_eq!(cosine_similarity(&a, &c), 1.0); // Identical vectors

        let d = vec![1.0, 1.0, 0.0];
        let similarity = cosine_similarity(&a, &d);
        assert!((similarity - 0.7071067).abs() < 0.0001); // 1/sqrt(2)
    }

    #[test]
    fn test_store_and_retrieve_embedding() {
        let (engine, _temp_dir) = setup_vector_engine();

        let embedding = vec![0.1, 0.2, 0.3, 0.4];
        let memory_id = "test_memory_1";
        let model_name = "test_model";

        // Store embedding
        engine
            .store_embedding(memory_id, &embedding, model_name)
            .unwrap();

        // Retrieve embedding
        let retrieved = engine.get_embedding(memory_id, model_name).unwrap();
        assert_eq!(retrieved, Some(embedding));

        // Test non-existent embedding
        let non_existent = engine.get_embedding("non_existent", model_name).unwrap();
        assert_eq!(non_existent, None);
    }

    #[test]
    fn test_vector_stats() {
        let (engine, _temp_dir) = setup_vector_engine();

        // Add some embeddings
        engine
            .store_embedding("mem1", &vec![0.1, 0.2, 0.3, 0.4], "model1")
            .unwrap();
        engine
            .store_embedding("mem2", &vec![0.5, 0.6, 0.7, 0.8], "model1")
            .unwrap();
        engine
            .store_embedding("mem3", &vec![0.9, 1.0, 1.1, 1.2], "model2")
            .unwrap();

        let stats = engine.get_vector_stats().unwrap();
        assert_eq!(stats.total_embeddings, 3);
        assert_eq!(stats.dimension, 4);
        assert_eq!(stats.models.get("model1"), Some(&2));
        assert_eq!(stats.models.get("model2"), Some(&1));
    }
}
