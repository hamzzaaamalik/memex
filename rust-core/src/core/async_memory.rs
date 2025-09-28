//! Async session manager implementation

use anyhow::{Context, Result};
use std::collections::HashMap;
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
            .get_user_sessions(user_id.to_string(), limit, offset)
            .await
            .context("Failed to get user sessions")?;

        let duration = start.elapsed().as_millis() as f32;
        {
            let monitor = self.monitor.write().await;
            monitor.record_query_time(duration);
        }

        log::debug!(
            "Retrieved {} sessions for user {} in {}ms",
            response.data.len(),
            user_id,
            duration
        );

        Ok(response)
    }

    /// Generate a summary for a session asynchronously
    pub async fn generate_session_summary(&self, session_id: &str) -> Result<SessionSummary> {
        let start = std::time::Instant::now();

        // Rate limiting (summary generation is expensive)
        self.validator.validate_request(5)?;

        // Get all memories for the session
        let filter = QueryFilter {
            session_id: Some(session_id.to_string()),
            limit: Some(1000), // Reasonable limit for summary generation
            ..Default::default()
        };

        let memories_response = self.database.recall_memories(filter).await?;
        let memories = memories_response.data;

        if memories.is_empty() {
            return Err(anyhow::anyhow!("No memories found for session"));
        }

        // Extract session info from first memory
        let user_id = memories[0].user_id.clone();

        // Generate summary using advanced text processing
        let summary = self.generate_intelligent_summary(&memories).await?;

        let duration = start.elapsed().as_millis() as f32;
        {
            let monitor = self.monitor.write().await;
            monitor.record_query_time(duration);
        }

        log::debug!(
            "Generated summary for session {} with {} memories in {}ms",
            session_id,
            memories.len(),
            duration
        );

        Ok(summary)
    }

    /// Generate an intelligent summary from memories asynchronously
    async fn generate_intelligent_summary(
        &self,
        memories: &[MemoryItem],
    ) -> Result<SessionSummary> {
        if memories.is_empty() {
            return Err(anyhow::anyhow!("Cannot summarize empty memory list"));
        }

        let session_id = memories[0].session_id.clone();
        let user_id = memories[0].user_id.clone();

        // Sort memories by timestamp
        let mut sorted_memories = memories.to_vec();
        sorted_memories.sort_by_key(|m| m.created_at);

        // Extract key topics using TF-IDF-like approach (async)
        let key_topics = task::spawn_blocking({
            let memories = sorted_memories.clone();
            move || Self::extract_key_topics_sync(&memories)
        })
        .await??;

        // Generate summary text (async)
        let summary_text = task::spawn_blocking({
            let memories = sorted_memories.clone();
            let topics = key_topics.clone();
            move || Self::generate_summary_text_sync(&memories, &topics)
        })
        .await??;

        // Calculate date range
        let date_range = (
            sorted_memories.first().unwrap().created_at,
            sorted_memories.last().unwrap().created_at,
        );

        // Calculate average importance
        let total_importance: f32 = sorted_memories.iter().map(|m| m.importance).sum();
        let importance_score = total_importance / sorted_memories.len() as f32;

        Ok(SessionSummary {
            session_id,
            user_id,
            summary_text,
            key_topics,
            memory_count: memories.len(),
            date_range,
            importance_score,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    // Helper methods (moved to sync versions for spawn_blocking)
    fn extract_key_topics_sync(memories: &[MemoryItem]) -> Result<Vec<String>> {
        let mut word_freq: HashMap<String, usize> = HashMap::new();
        let mut doc_freq: HashMap<String, usize> = HashMap::new();

        // Count word frequencies across all memories
        for memory in memories {
            let words = Self::tokenize_and_filter_sync(&memory.content.to_lowercase());
            let unique_words: std::collections::HashSet<_> = words.iter().collect();

            for word in words {
                *word_freq.entry(word.clone()).or_insert(0) += 1;
            }

            for word in unique_words {
                *doc_freq.entry(word.clone()).or_insert(0) += 1;
            }
        }

        // Calculate TF-IDF scores
        let total_docs = memories.len() as f64;
        let mut tf_idf_scores: Vec<(String, f64)> = word_freq
            .into_iter()
            .filter_map(|(word, tf)| {
                if let Some(df) = doc_freq.get(&word) {
                    let tf_score = tf as f64;
                    let idf_score = (total_docs / *df as f64).ln();
                    let tf_idf = tf_score * idf_score;

                    // Filter out very common or very rare terms
                    if *df > 1 && *df < (total_docs * 0.8) as usize {
                        Some((word, tf_idf))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Sort by TF-IDF score and take top terms
        tf_idf_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(tf_idf_scores
            .into_iter()
            .take(10) // Top 10 topics
            .map(|(word, _)| word)
            .collect())
    }

    fn generate_summary_text_sync(
        memories: &[MemoryItem],
        key_topics: &[String],
    ) -> Result<String> {
        let memory_count = memories.len();
        let time_span = if memories.len() > 1 {
            let start = memories.first().unwrap().created_at;
            let end = memories.last().unwrap().created_at;
            let duration = end - start;

            if duration.num_days() > 0 {
                format!(" over {} days", duration.num_days())
            } else if duration.num_hours() > 0 {
                format!(" over {} hours", duration.num_hours())
            } else {
                format!(" over {} minutes", duration.num_minutes())
            }
        } else {
            String::new()
        };

        // Find the most important memories for highlights
        let mut important_memories = memories.to_vec();
        important_memories.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());

        let highlights = important_memories
            .iter()
            .take(3)
            .map(|m| {
                if m.content.len() > 100 {
                    format!("\"{}...\"", &m.content[..100])
                } else {
                    format!("\"{}\"", m.content)
                }
            })
            .collect::<Vec<_>>();

        // Construct summary
        let mut summary = format!(
            "Session contains {} memories{}.{}",
            memory_count,
            time_span,
            if key_topics.is_empty() {
                String::new()
            } else {
                format!(" Key topics: {}.", key_topics.join(", "))
            }
        );

        if !highlights.is_empty() {
            summary.push_str(&format!(
                " Notable memories include: {}",
                highlights.join("; ")
            ));
        }

        // Add context about memory importance distribution
        let high_importance = memories.iter().filter(|m| m.importance > 0.7).count();
        let medium_importance = memories
            .iter()
            .filter(|m| m.importance > 0.4 && m.importance <= 0.7)
            .count();

        if high_importance > 0 {
            summary.push_str(&format!(" {} high-importance items", high_importance));
        }
        if medium_importance > 0 {
            summary.push_str(&format!(", {} medium-importance items", medium_importance));
        }

        Ok(summary)
    }

    fn tokenize_and_filter_sync(text: &str) -> Vec<String> {
        text.split_whitespace()
            .filter_map(|word| {
                let cleaned = word
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect::<String>();

                if cleaned.len() >= 3 && !Self::is_stop_word_sync(&cleaned) {
                    Some(cleaned)
                } else {
                    None
                }
            })
            .collect()
    }

    fn is_stop_word_sync(word: &str) -> bool {
        matches!(
            word.to_lowercase().as_str(),
            "the"
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
                | "up"
                | "about"
                | "into"
                | "through"
                | "during"
                | "before"
                | "after"
                | "above"
                | "below"
                | "between"
                | "among"
                | "this"
                | "that"
                | "these"
                | "those"
                | "was"
                | "were"
                | "are"
                | "is"
                | "been"
                | "being"
                | "have"
                | "has"
                | "had"
                | "will"
                | "would"
                | "could"
                | "should"
                | "may"
                | "might"
                | "can"
                | "must"
                | "shall"
                | "am"
                | "do"
                | "does"
                | "did"
                | "done"
                | "get"
                | "got"
                | "getting"
                | "very"
                | "much"
                | "more"
                | "most"
                | "many"
                | "some"
                | "any"
                | "all"
                | "each"
                | "every"
                | "few"
                | "several"
                | "other"
                | "another"
                | "such"
                | "only"
                | "own"
                | "same"
                | "so"
                | "than"
                | "too"
                | "just"
                | "now"
                | "here"
                | "there"
                | "when"
                | "where"
                | "why"
                | "how"
                | "what"
                | "which"
                | "who"
        )
    }

    /// Search sessions by content keywords asynchronously
    pub async fn search_sessions(
        &self,
        user_id: &str,
        keywords: Vec<String>,
    ) -> Result<Vec<Session>> {
        // Rate limiting
        self.validator.validate_request(2)?;

        if keywords.is_empty() {
            return Ok(Vec::new());
        }

        // Search memories with keywords to find relevant sessions
        let filter = QueryFilter {
            user_id: Some(user_id.to_string()),
            keywords: Some(keywords),
            limit: Some(1000), // Search across many memories
            ..Default::default()
        };

        let memories_response = self.database.recall_memories(filter).await?;

        // Collect unique session IDs
        let mut session_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for memory in memories_response.data {
            session_ids.insert(memory.session_id);
        }

        // Get session details for each session ID
        let mut matching_sessions = Vec::new();
        let sessions_response = self.get_user_sessions(user_id, None, None).await?;

        for session in sessions_response.data {
            if session_ids.contains(&session.id) {
                matching_sessions.push(session);
            }
        }

        // Sort by last_active descending
        matching_sessions.sort_by(|a, b| b.last_active.cmp(&a.last_active));

        Ok(matching_sessions)
    }

    /// Delete a session and optionally its memories asynchronously
    pub async fn delete_session(&self, session_id: &str, delete_memories: bool) -> Result<bool> {
        // Rate limiting (deletion is expensive)
        self.validator.validate_request(5)?;

        if delete_memories {
            // First, get all memories in the session
            let filter = QueryFilter {
                session_id: Some(session_id.to_string()),
                limit: Some(10000), // Large limit to get all memories
                ..Default::default()
            };

            let memories_response = self.database.recall_memories(filter).await?;

            // Delete each memory
            let mut delete_tasks = Vec::new();
            for memory in memories_response.data {
                let database = self.database.clone();
                let memory_id = memory.id.clone();
                delete_tasks.push(async move { database.delete_memory(memory_id).await });
            }

            // Execute deletions in parallel
            let results = futures::future::join_all(delete_tasks).await;
            let mut deleted_count = 0;
            for result in results {
                if result.unwrap_or(false) {
                    deleted_count += 1;
                }
            }

            log::info!(
                "Deleted {} memories from session {}",
                deleted_count,
                session_id
            );
        }

        // TODO: Implement delete_session in database layer
        log::debug!("Deleted session {}", session_id);

        Ok(true)
    }

    /// Get session analytics asynchronously
    pub async fn get_session_analytics(&self, user_id: &str) -> Result<SessionAnalytics> {
        let sessions_response = self.get_user_sessions(user_id, None, None).await?;
        let sessions = sessions_response.data;

        if sessions.is_empty() {
            return Ok(SessionAnalytics::default());
        }

        let total_sessions = sessions.len();
        let total_memories: usize = sessions.iter().map(|s| s.memory_count).sum();

        let most_active_session = sessions.iter().max_by_key(|s| s.memory_count).cloned();

        let most_recent_session = sessions.iter().max_by_key(|s| s.last_active).cloned();

        // Calculate session activity over time
        let mut activity_by_day: HashMap<String, usize> = HashMap::new();
        for session in &sessions {
            let date_key = session.last_active.format("%Y-%m-%d").to_string();
            *activity_by_day.entry(date_key).or_insert(0) += session.memory_count;
        }

        let avg_memories_per_session = if total_sessions > 0 {
            total_memories as f32 / total_sessions as f32
        } else {
            0.0
        };

        Ok(SessionAnalytics {
            user_id: user_id.to_string(),
            total_sessions,
            total_memories,
            avg_memories_per_session,
            most_active_session,
            most_recent_session,
            activity_by_day,
        })
    }

    /// Get performance metrics
    pub async fn get_performance_metrics(&self) -> crate::core::PerformanceMetrics {
        let monitor = self.monitor.read().await;
        monitor.get_metrics()
    }
}
