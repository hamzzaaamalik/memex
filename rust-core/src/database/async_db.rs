//! Async database operations for better Node.js integration

use anyhow::{Context, Result};
use tokio::task;
use std::sync::Arc;

use super::{Database, DatabaseConfig, models::*};
use super::vector::{VectorSearchEngine, VectorConfig, VectorSearchResult, HybridSearchResult};

/// Async wrapper for database operations
#[derive(Clone)]
pub struct AsyncDatabase {
    inner: Arc<Database>,
    vector_engine: Option<Arc<VectorSearchEngine>>,
}

impl AsyncDatabase {
    /// Create new async database instance
    pub async fn new(config: DatabaseConfig) -> Result<Self> {
        let inner = task::spawn_blocking(move || Database::new(config))
            .await
            .context("Failed to spawn database creation task")??;

        Ok(Self {
            inner: Arc::new(inner),
            vector_engine: None,
        })
    }

    /// Initialize with vector search support
    pub async fn new_with_vector(config: DatabaseConfig, vector_config: VectorConfig) -> Result<Self> {
        let database = Self::new(config).await?;
        
        let inner_clone = database.inner.clone();
        let vector_engine = task::spawn_blocking(move || {
            // Assuming we can extract a connection pool from Database
            // This would need to be implemented in the Database struct
            let pool = inner_clone.get_connection_pool(); // This method needs to be added
            let engine = VectorSearchEngine::new(pool, vector_config);
            engine.initialize_schema()?;
            Ok::<_, anyhow::Error>(Arc::new(engine))
        })
        .await
        .context("Failed to initialize vector engine")??;

        Ok(Self {
            inner: database.inner,
            vector_engine: Some(vector_engine),
        })
    }

    /// Async memory save operation
    pub async fn save_memory(&self, memory: MemoryItem) -> Result<String> {
        let db = self.inner.clone();
        task::spawn_blocking(move || db.save_memory(&memory))
            .await
            .context("Failed to spawn save memory task")?
    }

    /// Async memory recall operation
    pub async fn recall_memories(&self, filter: QueryFilter) -> Result<PaginatedResponse<MemoryItem>> {
        let db = self.inner.clone();
        task::spawn_blocking(move || db.recall_memories(&filter))
            .await
            .context("Failed to spawn recall memories task")?
    }

    /// Async memory retrieval by ID
    pub async fn get_memory(&self, id: String) -> Result<Option<MemoryItem>> {
        let db = self.inner.clone();
        task::spawn_blocking(move || db.get_memory(&id))
            .await
            .context("Failed to spawn get memory task")?
    }

    /// Async memory deletion
    pub async fn delete_memory(&self, id: String) -> Result<bool> {
        let db = self.inner.clone();
        task::spawn_blocking(move || db.delete_memory(&id))
            .await
            .context("Failed to spawn delete memory task")?
    }

    /// Async session creation
    pub async fn create_session(&self, user_id: String, name: Option<String>) -> Result<String> {
        let db = self.inner.clone();
        task::spawn_blocking(move || db.create_session(&user_id, name))
            .await
            .context("Failed to spawn create session task")?
    }

    /// Async user sessions retrieval
    pub async fn get_user_sessions(&self, user_id: String, limit: Option<usize>, offset: Option<usize>) -> Result<PaginatedResponse<Session>> {
        let db = self.inner.clone();
        task::spawn_blocking(move || db.get_user_sessions(&user_id, limit, offset))
            .await
            .context("Failed to spawn get user sessions task")?
    }

    /// Async database statistics
    pub async fn get_stats(&self) -> Result<serde_json::Value> {
        let db = self.inner.clone();
        task::spawn_blocking(move || db.get_stats())
            .await
            .context("Failed to spawn get stats task")?
    }

    /// Async expired memory cleanup
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let db = self.inner.clone();
        task::spawn_blocking(move || db.cleanup_expired())
            .await
            .context("Failed to spawn cleanup task")?
    }

    /// Async vector search (if vector engine is available)
    pub async fn search_similar(&self, query_embedding: Vec<f32>, model_name: String, limit: Option<usize>) -> Result<Vec<VectorSearchResult>> {
        match &self.vector_engine {
            Some(engine) => {
                let engine = engine.clone();
                task::spawn_blocking(move || engine.search_similar(&query_embedding, &model_name, limit))
                    .await
                    .context("Failed to spawn vector search task")?
            }
            None => Err(anyhow::anyhow!("Vector search engine not initialized"))
        }
    }

    /// Async hybrid search (text + vector)
    pub async fn hybrid_search(&self, text_query: String, vector_query: Vec<f32>, model_name: String, text_weight: f32, vector_weight: f32, limit: Option<usize>) -> Result<Vec<HybridSearchResult>> {
        match &self.vector_engine {
            Some(engine) => {
                let engine = engine.clone();
                task::spawn_blocking(move || {
                    engine.hybrid_search(&text_query, &vector_query, &model_name, text_weight, vector_weight, limit)
                })
                .await
                .context("Failed to spawn hybrid search task")?
            }
            None => Err(anyhow::anyhow!("Vector search engine not initialized"))
        }
    }

    /// Store embedding for a memory
    pub async fn store_embedding(&self, memory_id: String, embedding: Vec<f32>, model_name: String) -> Result<()> {
        match &self.vector_engine {
            Some(engine) => {
                let engine = engine.clone();
                task::spawn_blocking(move || engine.store_embedding(&memory_id, &embedding, &model_name))
                    .await
                    .context("Failed to spawn store embedding task")?
            }
            None => Err(anyhow::anyhow!("Vector search engine not initialized"))
        }
    }

    /// Batch memory operations
    pub async fn save_memories_batch(&self, memories: Vec<MemoryItem>) -> Result<Vec<Result<String, String>>> {
        let db = self.inner.clone();
        task::spawn_blocking(move || {
            let mut results = Vec::new();
            for memory in memories {
                match db.save_memory(&memory) {
                    Ok(id) => results.push(Ok(id)),
                    Err(e) => results.push(Err(e.to_string())),
                }
            }
            results
        })
        .await
        .context("Failed to spawn batch save task")
    }

    /// Stream-based memory processing for large datasets
    pub async fn save_memories_stream<S>(&self, mut stream: S) -> Result<Vec<Result<String, String>>>
    where
        S: futures::Stream<Item = MemoryItem> + Send + Unpin + 'static,
    {
        use futures::StreamExt;
        
        let db = self.inner.clone();
        let mut results = Vec::new();
        
        while let Some(memory) = stream.next().await {
            let db_clone = db.clone();
            let result = task::spawn_blocking(move || db_clone.save_memory(&memory))
                .await
                .context("Failed to spawn memory save task")?;
            
            match result {
                Ok(id) => results.push(Ok(id)),
                Err(e) => results.push(Err(e.to_string())),
            }
        }
        
        Ok(results)
    }

    /// Async memory export with progress tracking
    pub async fn export_user_memories_with_progress<F>(&self, user_id: String, progress_callback: F) -> Result<Vec<MemoryItem>>
    where
        F: Fn(usize, usize) + Send + Sync + 'static,
    {
        let db = self.inner.clone();
        let progress_callback = Arc::new(progress_callback);
        
        task::spawn_blocking(move || {
            // First, get total count
            let total_filter = QueryFilter {
                user_id: Some(user_id.clone()),
                limit: Some(1),
                ..Default::default()
            };
            let total_response = db.recall_memories(&total_filter)?;
            let total_count = total_response.total_count as usize;
            
            let mut all_memories = Vec::new();
            let mut offset = 0;
            let limit = 100; // Process in chunks
            
            loop {
                let filter = QueryFilter {
                    user_id: Some(user_id.clone()),
                    limit: Some(limit),
                    offset: Some(offset),
                    ..Default::default()
                };
                
                let response = db.recall_memories(&filter)?;
                if response.data.is_empty() {
                    break;
                }
                
                all_memories.extend(response.data);
                offset += limit;
                
                // Call progress callback
                progress_callback(all_memories.len(), total_count);
                
                if !response.has_next {
                    break;
                }
            }
            
            Ok(all_memories)
        })
        .await
        .context("Failed to spawn export task")?
    }

    /// Async transaction support
    pub async fn with_transaction<F, R>(&self, operation: F) -> Result<R>
    where
        F: FnOnce(&Database) -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let db = self.inner.clone();
        task::spawn_blocking(move || operation(&*db))
            .await
            .context("Failed to spawn transaction task")?
    }

    /// Check database health asynchronously
    pub async fn health_check(&self) -> Result<DatabaseHealth> {
        let db = self.inner.clone();
        let vector_engine = self.vector_engine.clone();
        
        task::spawn_blocking(move || {
            let mut health = DatabaseHealth {
                database_accessible: false,
                vector_search_available: false,
                pool_status: None,
                last_error: None,
            };
            
            // Test basic database access
            match db.get_stats() {
                Ok(_) => health.database_accessible = true,
                Err(e) => health.last_error = Some(e.to_string()),
            }
            
            // Test vector search if available
            if let Some(engine) = vector_engine {
                match engine.get_vector_stats() {
                    Ok(_) => health.vector_search_available = true,
                    Err(e) => {
                        if health.last_error.is_none() {
                            health.last_error = Some(format!("Vector search error: {}", e));
                        }
                    }
                }
            }
            
            // Get pool status
            health.pool_status = Some(db.get_pool_status());
            
            Ok(health)
        })
        .await
        .context("Failed to spawn health check task")?
    }
}

/// Database health information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatabaseHealth {
    pub database_accessible: bool,
    pub vector_search_available: bool,
    pub pool_status: Option<super::DatabasePoolStatus>,
    pub last_error: Option<String>,
}

impl DatabaseHealth {
    pub fn is_healthy(&self) -> bool {
        self.database_accessible && 
        self.pool_status.as_ref().map_or(true, |s| s.is_healthy()) &&
        self.last_error.is_none()
    }
}