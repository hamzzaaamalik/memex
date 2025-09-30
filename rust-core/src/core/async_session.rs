//! Async session manager implementation

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::task;

use crate::core::{PerformanceMonitor, RequestValidator};
use crate::database::async_db::AsyncDatabase;
use crate::database::models::*;

/// Async session manager for high-performance operations
pub struct AsyncSessionManager {
    database: AsyncDatabase,
    validator: Arc<RequestValidator>,
    monitor: Arc<tokio::sync::RwLock<PerformanceMonitor>>,
}

impl AsyncSessionManager {
    pub fn new(database: AsyncDatabase, validator: RequestValidator) -> Self {
        Self {
            database,
            validator: Arc::new(validator),
            monitor: Arc::new(tokio::sync::RwLock::new(PerformanceMonitor::new(1000))),
        }
    }

    /// Create a new session asynchronously
    pub async fn create_session(&self, user_id: &str, name: Option<String>) -> Result<String> {
        // Rate limiting
        self.validator.validate_request(1)?;

        // Validate user_id
        if user_id.trim().is_empty() || user_id.len() > 255 {
            return Err(anyhow::anyhow!("Invalid user_id"));
        }

        let session_id = self
            .database
            .create_session(user_id.to_string(), name)
            .await
            .context("Failed to create session")?;

        log::debug!("Created session {} for user {}", session_id, user_id);
        Ok(session_id)
    }

    /// Get sessions for a user with pagination asynchronously
    pub async fn get_user_sessions(
        &self,
        user_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<PaginatedResponse<Session>> {
        let start = std::time::Instant::now();

        // Rate limiting
        self.validator.validate_request(1)?;

        let response = self
            .database
            .get_user_sessions(user_id, limit, offset)
            .await
            .context("Failed to get user sessions")?;

        // Record performance
        let duration = start.elapsed().as_millis() as f32;
        let monitor = self.monitor.read().await;
        monitor.record_query_time(duration);

        Ok(response)
    }

    /// Generate session summary asynchronously
    pub async fn generate_session_summary(&self, session_id: &str) -> Result<SessionSummary> {
        let start = std::time::Instant::now();

        // Rate limiting
        self.validator.validate_request(5)?; // More expensive operation

        // Get all memories for this session
        let filter = QueryFilter {
            session_id: Some(session_id.to_string()),
            ..Default::default()
        };

        let memories_response = self
            .database
            .recall_memories(&filter)
            .await
            .context("Failed to recall memories for session")?;

        if memories_response.data.is_empty() {
            return Ok(SessionSummary {
                session_id: session_id.to_string(),
                summary_text: "No memories found for this session".to_string(),
                memory_count: 0,
                importance_score: 0.0,
                key_topics: Vec::new(),
                date_range: None,
                created_at: chrono::Utc::now(),
            });
        }

        // Calculate summary statistics
        let memory_count = memories_response.data.len();
        let avg_importance = memories_response
            .data
            .iter()
            .map(|m| m.importance)
            .sum::<f32>()
            / memory_count as f32;

        // Extract key topics (simple keyword extraction)
        let mut topic_frequency: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        for memory in &memories_response.data {
            for word in memory.content.split_whitespace() {
                let word = word.to_lowercase();
                if word.len() > 3 && !is_stop_word(&word) {
                    *topic_frequency.entry(word).or_insert(0) += 1;
                }
            }
        }

        let mut key_topics: Vec<String> = topic_frequency
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .map(|(word, _)| word)
            .collect();
        key_topics.sort();
        key_topics.truncate(10);

        // Generate summary text
        let summary_text = if memory_count == 1 {
            format!(
                "Session contains 1 memory with importance {:.2}",
                avg_importance
            )
        } else {
            format!(
                "Session contains {} memories with average importance {:.2}",
                memory_count, avg_importance
            )
        };

        // Calculate date range
        let dates: Vec<chrono::DateTime<chrono::Utc>> = memories_response
            .data
            .iter()
            .map(|m| m.created_at)
            .collect();
        let date_range = if dates.len() > 1 {
            let min_date = dates.iter().min().unwrap();
            let max_date = dates.iter().max().unwrap();
            Some(format!(
                "{} to {}",
                min_date.format("%Y-%m-%d"),
                max_date.format("%Y-%m-%d")
            ))
        } else {
            None
        };

        let summary = SessionSummary {
            session_id: session_id.to_string(),
            summary_text,
            memory_count: memory_count as i32,
            importance_score: avg_importance,
            key_topics,
            date_range,
            created_at: chrono::Utc::now(),
        };

        // Record performance
        let duration = start.elapsed().as_millis() as f32;
        let monitor = self.monitor.read().await;
        monitor.record_query_time(duration);

        Ok(summary)
    }

    /// Delete session and all associated memories asynchronously
    pub async fn delete_session(&self, user_id: &str, session_id: &str) -> Result<usize> {
        // Rate limiting
        self.validator.validate_request(10)?; // Very expensive operation

        let deleted_count = self
            .database
            .delete_session(user_id, session_id)
            .await
            .context("Failed to delete session")?;

        log::info!(
            "Deleted session {} for user {} ({} memories)",
            session_id,
            user_id,
            deleted_count
        );

        Ok(deleted_count)
    }

    /// Update session metadata asynchronously
    pub async fn update_session(&self, session_id: &str, name: Option<String>) -> Result<()> {
        // Rate limiting
        self.validator.validate_request(1)?;

        self.database
            .update_session(session_id, name)
            .await
            .context("Failed to update session")?;

        Ok(())
    }

    /// Get session analytics asynchronously
    pub async fn get_session_analytics(&self, user_id: &str) -> Result<SessionAnalytics> {
        let start = std::time::Instant::now();

        // Rate limiting
        self.validator.validate_request(5)?;

        let analytics = self
            .database
            .get_session_analytics(user_id)
            .await
            .context("Failed to get session analytics")?;

        // Record performance
        let duration = start.elapsed().as_millis() as f32;
        let monitor = self.monitor.read().await;
        monitor.record_query_time(duration);

        Ok(analytics)
    }
}

// Helper function to identify stop words
fn is_stop_word(word: &str) -> bool {
    matches!(
        word,
        "the"
            | "a"
            | "an"
            | "and"
            | "or"
            | "but"
            | "in"
            | "on"
            | "at"
            | "to"
            | "for"
            | "of"
            | "with"
            | "by"
            | "from"
            | "this"
            | "that"
            | "these"
            | "those"
            | "i"
            | "you"
            | "he"
            | "she"
            | "it"
            | "we"
            | "they"
            | "am"
            | "is"
            | "are"
            | "was"
            | "were"
            | "be"
            | "been"
            | "being"
            | "have"
            | "has"
            | "had"
            | "do"
            | "does"
            | "did"
            | "will"
            | "would"
            | "could"
            | "should"
            | "may"
            | "might"
            | "must"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::MemexConfig;
    use crate::database::async_db::AsyncDatabase;
    use tempfile::TempDir;

    async fn setup_test_manager() -> AsyncSessionManager {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = AsyncDatabase::new(&db_path.to_string_lossy())
            .await
            .unwrap();
        let config = MemexConfig::default();
        let validator = RequestValidator::new(&config);

        AsyncSessionManager::new(database, validator)
    }

    #[tokio::test]
    async fn test_create_session() {
        let manager = setup_test_manager().await;
        let session_id = manager
            .create_session("test_user", Some("Test Session".to_string()))
            .await
            .unwrap();
        assert!(!session_id.is_empty());
    }

    #[tokio::test]
    async fn test_invalid_user_id() {
        let manager = setup_test_manager().await;
        let result = manager.create_session("", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_user_sessions() {
        let manager = setup_test_manager().await;
        let session_id = manager
            .create_session("test_user", Some("Test Session".to_string()))
            .await
            .unwrap();

        let sessions = manager
            .get_user_sessions("test_user", Some(10), Some(0))
            .await
            .unwrap();
        assert_eq!(sessions.data.len(), 1);
        assert_eq!(sessions.data[0].id, session_id);
    }
}
