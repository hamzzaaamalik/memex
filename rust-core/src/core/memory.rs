//! Memory operations and management

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::time::Instant;
use uuid::Uuid;

use crate::core::{BatchRequest, BatchResponse, PerformanceMonitor, RequestValidator};
use crate::database::{models::*, Database};

/// Memory management service
pub struct MemoryManager {
    database: Database,
    validator: RequestValidator,
    monitor: PerformanceMonitor,
}

impl MemoryManager {
    pub fn new(database: Database, validator: RequestValidator) -> Self {
        Self {
            database,
            validator,
            monitor: PerformanceMonitor::new(1000), // Keep last 1000 samples
        }
    }

    /// Save a single memory item
    pub fn save_memory(&self, mut memory: MemoryItem) -> Result<String> {
        let start = Instant::now();

        // Rate limiting
        self.validator.validate_request(1)?;

        // Validation
        self.validator.validate_memory_item(&memory)?;

        // Set default values
        if memory.id.is_empty() {
            memory.id = Uuid::new_v4().to_string();
        }

        if memory.created_at == DateTime::<Utc>::MIN_UTC {
            memory.created_at = Utc::now();
        }

        memory.updated_at = Utc::now();

        // Calculate expiration
        if let Some(ttl_hours) = memory.ttl_hours {
            memory.expires_at = Some(memory.created_at + chrono::Duration::hours(ttl_hours as i64));
        }

        // Clamp importance
        memory.importance = memory.importance.clamp(0.0, 1.0);

        // Save to database
        let result = self
            .database
            .save_memory(&memory)
            .context("Failed to save memory to database");

        // Record performance
        let duration = start.elapsed().as_millis() as f32;
        self.monitor.record_save_time(duration);

        log::debug!("Saved memory {} in {}ms", memory.id, duration);
        result
    }

    /// Save multiple memories in a batch
    pub fn save_memories_batch(
        &self,
        request: BatchRequest<MemoryItem>,
    ) -> Result<BatchResponse<String>> {
        // Validate batch size
        self.validator.validate_batch_size(request.items.len())?;

        // Rate limiting (batch consumes more tokens)
        let batch_tokens = (request.items.len() / 10).max(1) as u32; // 1 token per 10 items
        self.validator.validate_request(batch_tokens)?;

        let mut response = BatchResponse::new();

        for memory in request.items {
            match self.save_memory(memory) {
                Ok(id) => response.add_success(id),
                Err(e) => {
                    let error_msg = e.to_string();
                    response.add_error(error_msg);

                    // If fail_on_error is true, stop processing
                    if request.fail_on_error {
                        break;
                    }
                }
            }
        }

        log::info!(
            "Batch save completed: {}/{} successful",
            response.success_count,
            response.results.len()
        );

        Ok(response)
    }

    /// Recall memories with filtering and pagination
    pub fn recall_memories(&self, filter: QueryFilter) -> Result<PaginatedResponse<MemoryItem>> {
        let start = Instant::now();

        // Rate limiting
        self.validator.validate_request(1)?;

        // Validation
        self.validator.validate_query_filter(&filter)?;

        // Execute query
        let result = self
            .database
            .recall_memories(&filter)
            .context("Failed to recall memories from database");

        // Record performance
        let duration = start.elapsed().as_millis() as f32;
        self.monitor.record_query_time(duration);

        match &result {
            Ok(response) => {
                log::debug!(
                    "Recalled {} memories in {}ms (page {}/{})",
                    response.data.len(),
                    duration,
                    response.page + 1,
                    response.total_pages
                );
            }
            Err(e) => {
                log::error!("Failed to recall memories: {}", e);
            }
        }

        result
    }

    /// Get a single memory by ID
    pub fn get_memory(&self, id: &str) -> Result<Option<MemoryItem>> {
        let start = Instant::now();

        // Rate limiting
        self.validator.validate_request(1)?;

        let result = self
            .database
            .get_memory(id)
            .context("Failed to get memory from database");

        let duration = start.elapsed().as_millis() as f32;
        self.monitor.record_query_time(duration);

        result
    }

    /// Update a memory item
    pub fn update_memory(&self, id: &str, updates: MemoryUpdate) -> Result<bool> {
        let start = Instant::now();

        // Rate limiting
        self.validator.validate_request(1)?;

        // Get existing memory
        let mut memory = match self.database.get_memory(id)? {
            Some(m) => m,
            None => return Ok(false), // Memory not found
        };

        // Apply updates
        if let Some(content) = updates.content {
            memory.content = content;
        }

        if let Some(importance) = updates.importance {
            memory.importance = importance.clamp(0.0, 1.0);
        }

        if let Some(metadata) = updates.metadata {
            memory.metadata = metadata;
        }

        if let Some(ttl_hours) = updates.ttl_hours {
            memory.ttl_hours = ttl_hours;
            memory.expires_at =
                ttl_hours.map(|ttl| memory.created_at + chrono::Duration::hours(ttl as i64));
        }

        memory.updated_at = Utc::now();

        // Validate updated memory
        self.validator.validate_memory_item(&memory)?;

        // Save updated memory
        self.database.save_memory(&memory)?;

        let duration = start.elapsed().as_millis() as f32;
        self.monitor.record_save_time(duration);

        log::debug!("Updated memory {} in {}ms", id, duration);
        Ok(true)
    }

    /// Delete a memory by ID
    pub fn delete_memory(&self, id: &str) -> Result<bool> {
        // Rate limiting
        self.validator.validate_request(1)?;

        let result = self
            .database
            .delete_memory(id)
            .context("Failed to delete memory from database");

        if let Ok(true) = result {
            log::debug!("Deleted memory {}", id);
        }

        result
    }

    /// Search memories using full-text search
    pub fn search_memories(
        &self,
        user_id: &str,
        query: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<PaginatedResponse<MemoryItem>> {
        let keywords = query
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        if keywords.is_empty() {
            return Ok(PaginatedResponse::empty());
        }

        let filter = QueryFilter {
            user_id: Some(user_id.to_string()),
            keywords: Some(keywords),
            limit,
            offset,
            ..Default::default()
        };

        self.recall_memories(filter)
    }

    /// Get memories for a specific session with pagination
    pub fn get_session_memories(
        &self,
        user_id: &str,
        session_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<PaginatedResponse<MemoryItem>> {
        let filter = QueryFilter::for_session(user_id, session_id);
        let filter = QueryFilter {
            limit,
            offset,
            ..filter
        };

        self.recall_memories(filter)
    }

    /// Get high-importance memories for a user
    pub fn get_important_memories(
        &self,
        user_id: &str,
        threshold: f32,
        limit: Option<usize>,
    ) -> Result<PaginatedResponse<MemoryItem>> {
        let filter = QueryFilter::high_importance(user_id, threshold);
        let filter = QueryFilter { limit, ..filter };

        self.recall_memories(filter)
    }

    /// Get memories within a date range
    pub fn get_memories_in_range(
        &self,
        user_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: Option<usize>,
    ) -> Result<PaginatedResponse<MemoryItem>> {
        let filter = QueryFilter::date_range(user_id, from, to);
        let filter = QueryFilter { limit, ..filter };

        self.recall_memories(filter)
    }

    /// Export all memories for a user
    pub fn export_user_memories(&self, user_id: &str) -> Result<Vec<MemoryItem>> {
        let mut all_memories = Vec::new();
        let mut offset = 0;
        let limit = 1000;

        loop {
            let filter = QueryFilter {
                user_id: Some(user_id.to_string()),
                limit: Some(limit),
                offset: Some(offset),
                ..Default::default()
            };

            let response = self.recall_memories(filter)?;
            all_memories.extend(response.data);

            if !response.has_next {
                break;
            }

            offset += limit;
        }

        log::info!(
            "Exported {} memories for user {}",
            all_memories.len(),
            user_id
        );
        Ok(all_memories)
    }

    /// Get memory statistics for a user
    pub fn get_user_memory_stats(&self, user_id: &str) -> Result<UserMemoryStats> {
        let filter = QueryFilter {
            user_id: Some(user_id.to_string()),
            limit: Some(1), // We just want the count
            ..Default::default()
        };

        let response = self.recall_memories(filter)?;

        // Get importance distribution
        let all_memories = self.export_user_memories(user_id)?;

        let mut importance_buckets = HashMap::new();
        let mut total_importance = 0.0;

        for memory in &all_memories {
            let bucket = match memory.importance {
                i if i >= 0.8 => "high",
                i if i >= 0.5 => "medium",
                i if i >= 0.2 => "low",
                _ => "very_low",
            };

            *importance_buckets.entry(bucket.to_string()).or_insert(0) += 1;
            total_importance += memory.importance;
        }

        let avg_importance = if all_memories.is_empty() {
            0.0
        } else {
            total_importance / all_memories.len() as f32
        };

        // Calculate age distribution
        let now = Utc::now();
        let mut age_buckets = HashMap::new();

        for memory in &all_memories {
            let age_hours = (now - memory.created_at).num_hours();
            let bucket = match age_hours {
                h if h <= 24 => "24h",
                h if h <= 168 => "1week",  // 7 days
                h if h <= 720 => "1month", // 30 days
                h if h <= 8760 => "1year", // 365 days
                _ => "older",
            };

            *age_buckets.entry(bucket.to_string()).or_insert(0) += 1;
        }

        Ok(UserMemoryStats {
            user_id: user_id.to_string(),
            total_memories: response.total_count,
            avg_importance,
            importance_distribution: importance_buckets,
            age_distribution: age_buckets,
            oldest_memory: all_memories
                .iter()
                .min_by_key(|m| m.created_at)
                .map(|m| m.created_at),
            newest_memory: all_memories
                .iter()
                .max_by_key(|m| m.created_at)
                .map(|m| m.created_at),
        })
    }

    /// Get performance metrics
    pub fn get_performance_metrics(&self) -> crate::core::PerformanceMetrics {
        self.monitor.get_metrics()
    }

    /// Reset performance monitoring
    pub fn reset_performance_monitoring(&self) {
        self.monitor.reset();
    }
}

/// Memory update request
#[derive(Debug, Clone)]
pub struct MemoryUpdate {
    pub content: Option<String>,
    pub importance: Option<f32>,
    pub metadata: Option<HashMap<String, String>>,
    pub ttl_hours: Option<Option<u32>>, // None = no change, Some(None) = remove TTL, Some(Some(x)) = set TTL
}

/// User memory statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserMemoryStats {
    pub user_id: String,
    pub total_memories: i64,
    pub avg_importance: f32,
    pub importance_distribution: HashMap<String, i32>,
    pub age_distribution: HashMap<String, i32>,
    pub oldest_memory: Option<DateTime<Utc>>,
    pub newest_memory: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MemexConfig, RequestValidator};
    use crate::database::{Database, DatabaseConfig};
    use tempfile::TempDir;

    fn setup_test_manager() -> (MemoryManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_config = DatabaseConfig {
            path: temp_dir
                .path()
                .join("test.db")
                .to_string_lossy()
                .to_string(),
            ..Default::default()
        };

        let database = Database::new(db_config).unwrap();
        let config = MemexConfig::default();
        let validator = RequestValidator::new(&config);
        let manager = MemoryManager::new(database, validator);

        (manager, temp_dir)
    }

    #[test]
    fn test_save_and_recall_memory() {
        let (manager, _temp_dir) = setup_test_manager();

        let memory = MemoryItem {
            user_id: "test_user".to_string(),
            session_id: "test_session".to_string(),
            content: "Test memory content".to_string(),
            importance: 0.8,
            ..Default::default()
        };

        // Save memory
        let memory_id = manager.save_memory(memory.clone()).unwrap();
        assert!(!memory_id.is_empty());

        // Recall memory
        let filter = QueryFilter::for_user_with_keywords("test_user", vec!["Test".to_string()]);
        let response = manager.recall_memories(filter).unwrap();

        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].content, "Test memory content");
        assert_eq!(response.data[0].importance, 0.8);
    }

    #[test]
    fn test_batch_save_memories() {
        let (manager, _temp_dir) = setup_test_manager();

        let memories = vec![
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "test_session".to_string(),
                content: "Memory 1".to_string(),
                ..Default::default()
            },
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "test_session".to_string(),
                content: "Memory 2".to_string(),
                ..Default::default()
            },
        ];

        let request = BatchRequest {
            items: memories,
            fail_on_error: false,
        };

        let response = manager.save_memories_batch(request).unwrap();
        assert_eq!(response.success_count, 2);
        assert_eq!(response.error_count, 0);
    }

    #[test]
    fn test_search_memories() {
        let (manager, _temp_dir) = setup_test_manager();

        // Save some test memories
        let memories = vec![
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "Trading AAPL stocks today".to_string(),
                ..Default::default()
            },
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "Bitcoin price analysis".to_string(),
                ..Default::default()
            },
        ];

        for memory in memories {
            manager.save_memory(memory).unwrap();
        }

        // Search for "AAPL"
        let response = manager
            .search_memories("test_user", "AAPL", Some(10), Some(0))
            .unwrap();
        assert_eq!(response.data.len(), 1);
        assert!(response.data[0].content.contains("AAPL"));

        // Search for "price"
        let response = manager
            .search_memories("test_user", "price", Some(10), Some(0))
            .unwrap();
        assert_eq!(response.data.len(), 1);
        assert!(response.data[0].content.contains("Bitcoin"));
    }

    #[test]
    fn test_memory_update() {
        let (manager, _temp_dir) = setup_test_manager();

        let memory = MemoryItem {
            user_id: "test_user".to_string(),
            session_id: "test_session".to_string(),
            content: "Original content".to_string(),
            importance: 0.5,
            ..Default::default()
        };

        let memory_id = manager.save_memory(memory).unwrap();

        // Update memory
        let update = MemoryUpdate {
            content: Some("Updated content".to_string()),
            importance: Some(0.9),
            metadata: None,
            ttl_hours: None,
        };

        let updated = manager.update_memory(&memory_id, update).unwrap();
        assert!(updated);

        // Verify update
        let retrieved = manager.get_memory(&memory_id).unwrap().unwrap();
        assert_eq!(retrieved.content, "Updated content");
        assert_eq!(retrieved.importance, 0.9);
    }

    #[test]
    fn test_user_memory_stats() {
        let (manager, _temp_dir) = setup_test_manager();

        let memories = vec![
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "High importance memory".to_string(),
                importance: 0.9,
                ..Default::default()
            },
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "Low importance memory".to_string(),
                importance: 0.1,
                ..Default::default()
            },
        ];

        for memory in memories {
            manager.save_memory(memory).unwrap();
        }

        let stats = manager.get_user_memory_stats("test_user").unwrap();
        assert_eq!(stats.total_memories, 2);
        assert_eq!(stats.avg_importance, 0.5);
        assert!(stats.importance_distribution.contains_key("high"));
        assert!(stats.importance_distribution.contains_key("very_low"));
    }

    #[test]
    fn test_performance_monitoring() {
        let (manager, _temp_dir) = setup_test_manager();

        // Perform some operations to generate metrics
        let memory = MemoryItem {
            user_id: "test_user".to_string(),
            session_id: "test_session".to_string(),
            content: "Test content".to_string(),
            ..Default::default()
        };

        manager.save_memory(memory).unwrap();

        let filter = QueryFilter {
            user_id: Some("test_user".to_string()),
            ..Default::default()
        };
        manager.recall_memories(filter).unwrap();

        let metrics = manager.get_performance_metrics();
        assert!(metrics.avg_save_time_ms > 0.0);
        assert!(metrics.avg_query_time_ms > 0.0);
    }
}
