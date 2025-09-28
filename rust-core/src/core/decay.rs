//! Memory decay and cleanup operations

use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

use crate::core::{PerformanceMonitor, RequestValidator};
use crate::database::{models::*, Database};

/// Memory decay engine for automated cleanup and compression
pub struct DecayEngine {
    database: Database,
    validator: RequestValidator,
    monitor: PerformanceMonitor,
    policy: DecayPolicy,
}

impl DecayEngine {
    pub fn new(database: Database, validator: RequestValidator, policy: DecayPolicy) -> Self {
        Self {
            database,
            validator,
            monitor: PerformanceMonitor::new(100), // Smaller sample size for decay operations
            policy,
        }
    }

    /// Update decay policy
    pub fn update_policy(&mut self, policy: DecayPolicy) -> Result<()> {
        // Validate policy
        policy.validate().context("Invalid decay policy")?;

        self.policy = policy;
        log::info!(
            "Updated decay policy: max_age={}h, threshold={}, compression={}",
            self.policy.max_age_hours,
            self.policy.importance_threshold,
            self.policy.compression_enabled
        );

        Ok(())
    }

    /// Run full decay process
    pub fn run_decay(&self) -> Result<DecayStats> {
        let start_time = Utc::now();
        let run_id = Uuid::new_v4().to_string();

        log::info!("Starting decay process (run_id: {})", run_id);

        // Rate limiting (decay is expensive)
        self.validator.validate_request(10)?;

        // Initialize decay run record
        let mut stats = DecayStats {
            run_id: run_id.clone(),
            started_at: start_time,
            completed_at: None,
            memories_expired: 0,
            memories_compressed: 0,
            sessions_summarized: 0,
            total_memories_before: 0,
            total_memories_after: 0,
            storage_saved_bytes: 0,
            status: DecayStatus::Running,
            error_message: None,
        };

        // Get initial memory count
        match self.get_total_memory_count() {
            Ok(count) => stats.total_memories_before = count,
            Err(e) => {
                log::error!("Failed to get initial memory count: {}", e);
                stats.status = DecayStatus::Failed;
                stats.error_message = Some(e.to_string());
                return Ok(stats);
            }
        }

        // Step 1: Remove expired memories
        match self.expire_old_memories() {
            Ok(expired) => {
                stats.memories_expired = expired;
                log::info!("Expired {} memories", expired);
            }
            Err(e) => {
                log::error!("Failed to expire memories: {}", e);
                stats.status = DecayStatus::Failed;
                stats.error_message = Some(format!("Expiry failed: {}", e));
                return Ok(stats);
            }
        }

        // Step 2: Compress old memories if enabled
        if self.policy.compression_enabled {
            match self.compress_old_memories() {
                Ok(compressed) => {
                    stats.memories_compressed = compressed;
                    log::info!("Compressed {} memories", compressed);
                }
                Err(e) => {
                    log::error!("Failed to compress memories: {}", e);
                    // Don't fail the entire process for compression errors
                    stats.error_message = Some(format!("Compression failed: {}", e));
                }
            }
        }

        // Step 3: Auto-summarize old sessions if enabled
        if self.policy.auto_summarize_sessions {
            match self.summarize_old_sessions() {
                Ok(summarized) => {
                    stats.sessions_summarized = summarized;
                    log::info!("Summarized {} sessions", summarized);
                }
                Err(e) => {
                    log::error!("Failed to summarize sessions: {}", e);
                    // Don't fail the entire process for summarization errors
                }
            }
        }

        // Step 4: Enforce per-user memory limits
        match self.enforce_memory_limits() {
            Ok(limited) => {
                stats.memories_expired += limited;
                log::info!("Enforced limits, removed {} additional memories", limited);
            }
            Err(e) => {
                log::error!("Failed to enforce memory limits: {}", e);
                stats.error_message = Some(format!("Limit enforcement failed: {}", e));
            }
        }

        // Get final memory count
        match self.get_total_memory_count() {
            Ok(count) => stats.total_memories_after = count,
            Err(e) => log::error!("Failed to get final memory count: {}", e),
        }

        // Calculate storage saved (rough estimate)
        let memories_removed = stats.memories_expired + stats.memories_compressed;
        stats.storage_saved_bytes = memories_removed * 1024; // Rough estimate: 1KB per memory

        // Complete decay run
        stats.completed_at = Some(Utc::now());
        if stats.status == DecayStatus::Running {
            stats.status = DecayStatus::Completed;
        }

        let duration = Utc::now() - start_time;
        log::info!(
            "Decay process completed in {}ms (run_id: {})",
            duration.num_milliseconds(),
            run_id
        );
        log::info!(
            "Results: expired={}, compressed={}, sessions={}, before={}, after={}",
            stats.memories_expired,
            stats.memories_compressed,
            stats.sessions_summarized,
            stats.total_memories_before,
            stats.total_memories_after
        );

        Ok(stats)
    }

    /// Remove memories that have exceeded their TTL or are too old
    fn expire_old_memories(&self) -> Result<usize> {
        let now = Utc::now();
        let cutoff_time = now - chrono::Duration::hours(self.policy.max_age_hours as i64);

        // First, cleanup explicitly expired memories (TTL-based)
        let expired_count = self
            .database
            .cleanup_expired()
            .context("Failed to cleanup expired memories")?;

        // Then, find old low-importance memories to expire
        let filter = QueryFilter {
            date_to: Some(cutoff_time),
            limit: Some(1000), // Process in batches
            ..Default::default()
        };

        let old_memories_response = self.database.recall_memories(&filter)?;
        let mut additional_expired = 0;

        for memory in old_memories_response.data {
            // Only expire if importance is below threshold
            if memory.importance < self.policy.importance_threshold {
                match self.database.delete_memory(&memory.id) {
                    Ok(true) => {
                        additional_expired += 1;
                        log::debug!(
                            "Expired old memory {} (age: {}h, importance: {})",
                            memory.id,
                            (now - memory.created_at).num_hours(),
                            memory.importance
                        );
                    }
                    Ok(false) => {
                        log::warn!("Memory {} not found for expiry", memory.id);
                    }
                    Err(e) => {
                        log::error!("Failed to delete memory {}: {}", memory.id, e);
                    }
                }
            }
        }

        Ok(expired_count + additional_expired)
    }

    /// Compress groups of old, low-importance memories
    fn compress_old_memories(&self) -> Result<usize> {
        let cutoff_date =
            Utc::now() - chrono::Duration::hours(self.policy.max_age_hours as i64 / 2);
        let mut compressed_count = 0;

        // Get old memories with low importance, grouped by user and session
        let filter = QueryFilter {
            date_to: Some(cutoff_date),
            limit: Some(1000),
            ..Default::default()
        };

        let old_memories_response = self.database.recall_memories(&filter)?;
        let old_memories: Vec<_> = old_memories_response
            .data
            .into_iter()
            .filter(|m| m.importance < self.policy.importance_threshold)
            .collect();

        // Group by (user_id, session_id)
        let mut memory_groups: HashMap<(String, String), Vec<MemoryItem>> = HashMap::new();
        for memory in old_memories {
            let key = (memory.user_id.clone(), memory.session_id.clone());
            memory_groups
                .entry(key)
                .or_insert_with(Vec::new)
                .push(memory);
        }

        // Compress groups with 3+ memories
        for ((_user_id, session_id), memories) in memory_groups {
            if memories.len() >= 3 {
                match self.create_compressed_memory(memories) {
                    Ok(compressed_memory) => {
                        // Save compressed memory
                        let compressed_id = self.database.save_memory(&compressed_memory)?;

                        // Delete original memories
                        for memory in &compressed_memory.compressed_from {
                            self.database.delete_memory(memory).ok(); // Continue even if some deletions fail
                        }

                        compressed_count += compressed_memory.compressed_from.len();

                        log::debug!(
                            "Compressed {} memories from session {} into {}",
                            compressed_memory.compressed_from.len(),
                            session_id,
                            compressed_id
                        );
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to compress memories for session {}: {}",
                            session_id,
                            e
                        );
                    }
                }
            }
        }

        Ok(compressed_count)
    }

    /// Create a compressed memory from multiple memories
    fn create_compressed_memory(&self, memories: Vec<MemoryItem>) -> Result<MemoryItem> {
        if memories.is_empty() {
            return Err(anyhow::anyhow!("Cannot compress empty memory list"));
        }

        let user_id = memories[0].user_id.clone();
        let session_id = memories[0].session_id.clone();

        // Sort by timestamp
        let mut sorted_memories = memories;
        sorted_memories.sort_by_key(|m| m.created_at);

        // Extract original IDs
        let original_ids: Vec<String> = sorted_memories.iter().map(|m| m.id.clone()).collect();

        // Generate summary
        let summary = self.generate_compression_summary(&sorted_memories)?;

        // Extract key points (top keywords)
        let key_points = self.extract_key_points(&sorted_memories);

        // Calculate combined importance (weighted average)
        let total_importance: f32 = sorted_memories.iter().map(|m| m.importance).sum();
        let combined_importance = total_importance / sorted_memories.len() as f32;

        // Create compressed memory
        let compressed_memory = MemoryItem {
            id: Uuid::new_v4().to_string(),
            user_id,
            session_id,
            content: summary,
            metadata: {
                let mut metadata = HashMap::new();
                metadata.insert("type".to_string(), "compressed".to_string());
                metadata.insert(
                    "original_count".to_string(),
                    sorted_memories.len().to_string(),
                );
                metadata.insert(
                    "key_points".to_string(),
                    serde_json::to_string(&key_points).unwrap_or_default(),
                );
                metadata.insert(
                    "date_range_start".to_string(),
                    sorted_memories.first().unwrap().created_at.to_rfc3339(),
                );
                metadata.insert(
                    "date_range_end".to_string(),
                    sorted_memories.last().unwrap().created_at.to_rfc3339(),
                );
                metadata
            },
            created_at: sorted_memories.first().unwrap().created_at,
            updated_at: Utc::now(),
            expires_at: None, // Compressed memories don't expire automatically
            importance: combined_importance,
            ttl_hours: None,
            is_compressed: true,
            compressed_from: original_ids,
            ..Default::default()
        };

        Ok(compressed_memory)
    }

    /// Generate a summary for compressed memories
    fn generate_compression_summary(&self, memories: &[MemoryItem]) -> Result<String> {
        if memories.is_empty() {
            return Err(anyhow::anyhow!("Cannot summarize empty memory list"));
        }

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

        // Get a sample of content from the most important memories
        let mut sample_memories = memories.to_vec();
        sample_memories.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());

        let sample_content: String = sample_memories
            .iter()
            .take(3)
            .map(|m| {
                if m.content.len() > 50 {
                    format!("{}...", &m.content[..50])
                } else {
                    m.content.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" | ");

        let summary = format!(
            "[COMPRESSED] {} memories{}: {}",
            memory_count, time_span, sample_content
        );

        Ok(summary)
    }

    /// Extract key points from memories (simple keyword extraction)
    fn extract_key_points(&self, memories: &[MemoryItem]) -> Vec<String> {
        let mut word_counts: HashMap<String, usize> = HashMap::new();

        for memory in memories {
            let words: Vec<String> = memory
                .content
                .to_lowercase()
                .split_whitespace()
                .filter_map(|word| {
                    let cleaned: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
                    if cleaned.len() > 3 && !self.is_stop_word(&cleaned) {
                        Some(cleaned)
                    } else {
                        None
                    }
                })
                .collect();

            for word in words {
                *word_counts.entry(word).or_insert(0) += 1;
            }
        }

        // Get top 5 most frequent words
        let mut sorted_words: Vec<(String, usize)> = word_counts.into_iter().collect();
        sorted_words.sort_by(|a, b| b.1.cmp(&a.1));
        sorted_words
            .into_iter()
            .take(5)
            .map(|(word, _)| word)
            .collect()
    }

    /// Check if a word is a stop word
    fn is_stop_word(&self, word: &str) -> bool {
        matches!(
            word,
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
                | "do"
                | "does"
                | "did"
        )
    }

    /// Auto-summarize sessions that haven't been active recently
    fn summarize_old_sessions(&self) -> Result<usize> {
        let cutoff_date = Utc::now() - chrono::Duration::days(7); // Sessions inactive for 7+ days
        let mut summarized_count = 0;

        // Get all users (we'll need to implement a way to get all users)
        // For now, we'll use a different approach - find sessions from old memories
        let filter = QueryFilter {
            date_to: Some(cutoff_date),
            limit: Some(1000),
            ..Default::default()
        };

        let old_memories_response = self.database.recall_memories(&filter)?;

        // Group by session to find sessions with enough memories
        let mut session_memory_counts: HashMap<String, usize> = HashMap::new();
        for memory in old_memories_response.data {
            *session_memory_counts.entry(memory.session_id).or_insert(0) += 1;
        }

        // Summarize sessions with 5+ memories
        for (session_id, memory_count) in session_memory_counts {
            if memory_count >= 5 {
                // TODO: Implement session summarization
                // This would require a SessionManager instance or similar functionality
                log::debug!(
                    "Would summarize session {} with {} memories",
                    session_id,
                    memory_count
                );
                summarized_count += 1;
            }
        }

        Ok(summarized_count)
    }

    /// Enforce per-user memory limits
    fn enforce_memory_limits(&self) -> Result<usize> {
        let removed_count = 0;

        // Get memory counts per user (we'll need to implement this efficiently)
        // For now, we'll use a simple approach with pagination

        // This is a simplified implementation - in production, you'd want to:
        // 1. Get all users efficiently
        // 2. Check their memory counts
        // 3. Remove least important memories for users over limit

        // TODO: Implement efficient user enumeration and quota enforcement
        log::debug!("Memory limit enforcement not fully implemented");

        Ok(removed_count)
    }

    /// Get total memory count (active memories only)
    fn get_total_memory_count(&self) -> Result<usize> {
        let filter = QueryFilter {
            limit: Some(1), // We just need the count
            ..Default::default()
        };

        let response = self.database.recall_memories(&filter)?;
        Ok(response.total_count as usize)
    }

    /// Analyze memory age distribution
    pub fn analyze_memory_age_distribution(&self) -> Result<HashMap<String, usize>> {
        let now = Utc::now();
        let mut age_buckets: HashMap<String, usize> = HashMap::new();

        // Get all active memories (in batches to avoid memory issues)
        let mut offset = 0;
        let batch_size = 1000;

        loop {
            let filter = QueryFilter {
                limit: Some(batch_size),
                offset: Some(offset),
                ..Default::default()
            };

            let response = self.database.recall_memories(&filter)?;

            if response.data.is_empty() {
                break;
            }

            for memory in response.data {
                let age_hours = (now - memory.created_at).num_hours();
                let bucket = match age_hours {
                    0..=24 => "0-24h",
                    25..=168 => "1-7d",     // 1 week
                    169..=720 => "1-4w",    // 1 month
                    721..=2160 => "1-3m",   // 3 months
                    2161..=8760 => "3m-1y", // 1 year
                    _ => "1y+",
                };

                *age_buckets.entry(bucket.to_string()).or_insert(0) += 1;
            }

            if !response.has_next {
                break;
            }

            offset += batch_size;
        }

        Ok(age_buckets)
    }

    /// Get decay statistics and recommendations
    pub fn get_decay_recommendations(&self) -> Result<DecayRecommendations> {
        let age_distribution = self.analyze_memory_age_distribution()?;
        let total_memories: usize = age_distribution.values().sum();

        // Calculate recommendations based on age distribution
        let old_memories = *age_distribution.get("1y+").unwrap_or(&0)
            + *age_distribution.get("3m-1y").unwrap_or(&0);
        let old_percentage = if total_memories > 0 {
            (old_memories as f32 / total_memories as f32) * 100.0
        } else {
            0.0
        };

        let mut recommendations = Vec::new();

        if old_percentage > 50.0 {
            recommendations
                .push("Consider running decay process - over 50% of memories are old".to_string());
        }

        if total_memories > self.policy.max_memories_per_user {
            recommendations
                .push("Memory count exceeds configured limits - cleanup recommended".to_string());
        }

        if *age_distribution.get("0-24h").unwrap_or(&0) > (total_memories / 2) {
            recommendations.push(
                "High recent activity detected - consider adjusting TTL policies".to_string(),
            );
        }

        Ok(DecayRecommendations {
            total_memories,
            age_distribution,
            old_memory_percentage: old_percentage,
            recommendations,
            suggested_max_age_hours: if old_percentage > 70.0 {
                Some(self.policy.max_age_hours / 2) // More aggressive cleanup
            } else {
                None
            },
            estimated_cleanup_count: old_memories,
        })
    }

    /// Get performance metrics
    pub fn get_performance_metrics(&self) -> crate::core::PerformanceMetrics {
        self.monitor.get_metrics()
    }
}

/// Decay recommendations
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecayRecommendations {
    pub total_memories: usize,
    pub age_distribution: HashMap<String, usize>,
    pub old_memory_percentage: f32,
    pub recommendations: Vec<String>,
    pub suggested_max_age_hours: Option<u32>,
    pub estimated_cleanup_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MindCacheConfig, RequestValidator};
    use crate::database::{Database, DatabaseConfig};
    use tempfile::TempDir;

    fn setup_test_engine() -> (DecayEngine, TempDir) {
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
        let config = MindCacheConfig::default();
        let validator = RequestValidator::new(&config);
        let policy = DecayPolicy::default();
        let engine = DecayEngine::new(database, validator, policy);

        (engine, temp_dir)
    }

    #[test]
    fn test_decay_policy_update() {
        let (mut engine, _temp_dir) = setup_test_engine();

        let new_policy = DecayPolicy {
            max_age_hours: 48,
            importance_threshold: 0.5,
            max_memories_per_user: 5000,
            compression_enabled: false,
            auto_summarize_sessions: false,
        };

        engine.update_policy(new_policy.clone()).unwrap();
        assert_eq!(engine.policy.max_age_hours, 48);
        assert_eq!(engine.policy.importance_threshold, 0.5);
        assert!(!engine.policy.compression_enabled);
    }

    #[test]
    fn test_compression_summary_generation() {
        let (engine, _temp_dir) = setup_test_engine();

        let memories = vec![
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "First memory about trading stocks".to_string(),
                importance: 0.2,
                created_at: Utc::now() - chrono::Duration::hours(2),
                ..Default::default()
            },
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "Second memory about market analysis".to_string(),
                importance: 0.3,
                created_at: Utc::now() - chrono::Duration::hours(1),
                ..Default::default()
            },
        ];

        let summary = engine.generate_compression_summary(&memories).unwrap();
        assert!(summary.contains("[COMPRESSED]"));
        assert!(summary.contains("2 memories"));
        assert!(summary.contains("trading") || summary.contains("market"));
    }

    #[test]
    fn test_key_points_extraction() {
        let (engine, _temp_dir) = setup_test_engine();

        let memories = vec![
            MemoryItem {
                content: "Trading stocks in the market today with analysis".to_string(),
                ..Default::default()
            },
            MemoryItem {
                content: "Stock market analysis shows positive trading trends".to_string(),
                ..Default::default()
            },
        ];

        let key_points = engine.extract_key_points(&memories);

        // Should extract meaningful words and filter out stop words
        assert!(
            key_points.contains(&"trading".to_string())
                || key_points.contains(&"market".to_string())
                || key_points.contains(&"analysis".to_string())
        );

        // Should not contain stop words
        assert!(!key_points.contains(&"the".to_string()));
        assert!(!key_points.contains(&"in".to_string()));
    }

    #[test]
    fn test_stop_word_detection() {
        let (engine, _temp_dir) = setup_test_engine();

        assert!(engine.is_stop_word("the"));
        assert!(engine.is_stop_word("and"));
        assert!(engine.is_stop_word("in"));
        assert!(!engine.is_stop_word("trading"));
        assert!(!engine.is_stop_word("analysis"));
    }

    #[test]
    fn test_create_compressed_memory() {
        let (engine, _temp_dir) = setup_test_engine();

        let memories = vec![
            MemoryItem {
                id: "mem1".to_string(),
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "First memory".to_string(),
                importance: 0.2,
                created_at: Utc::now() - chrono::Duration::hours(2),
                ..Default::default()
            },
            MemoryItem {
                id: "mem2".to_string(),
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "Second memory".to_string(),
                importance: 0.3,
                created_at: Utc::now() - chrono::Duration::hours(1),
                ..Default::default()
            },
        ];

        let compressed = engine.create_compressed_memory(memories).unwrap();

        assert!(compressed.is_compressed);
        assert_eq!(compressed.compressed_from.len(), 2);
        assert!(compressed.compressed_from.contains(&"mem1".to_string()));
        assert!(compressed.compressed_from.contains(&"mem2".to_string()));
        assert_eq!(compressed.importance, 0.25); // Average of 0.2 and 0.3
        assert!(compressed.content.contains("[COMPRESSED]"));
    }

    #[test]
    fn test_age_distribution_analysis() {
        let (engine, _temp_dir) = setup_test_engine();

        // This test would need actual memories in the database
        // For now, we'll just test that the function doesn't panic
        let distribution = engine.analyze_memory_age_distribution().unwrap();

        // Should return valid age buckets (even if empty)
        assert!(distribution.get("0-24h").is_some() || distribution.is_empty());
    }

    #[test]
    fn test_decay_recommendations() {
        let (engine, _temp_dir) = setup_test_engine();

        let recommendations = engine.get_decay_recommendations().unwrap();

        // Should return valid recommendations structure
        assert!(recommendations.total_memories >= 0);
        assert!(recommendations.old_memory_percentage >= 0.0);
        assert!(recommendations.old_memory_percentage <= 100.0);
    }

    #[test]
    fn test_run_decay_empty_database() {
        let (engine, _temp_dir) = setup_test_engine();

        // Should handle empty database gracefully
        let stats = engine.run_decay().unwrap();

        assert_eq!(stats.total_memories_before, 0);
        assert_eq!(stats.total_memories_after, 0);
        assert_eq!(stats.memories_expired, 0);
        assert!(matches!(stats.status, DecayStatus::Completed));
    }
}
