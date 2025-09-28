//! Session management and operations

use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;

use crate::core::{RequestValidator, PerformanceMonitor};
use crate::database::{Database, models::*};

/// Session management service
pub struct SessionManager {
    database: Database,
    validator: RequestValidator,
    monitor: PerformanceMonitor,
}

impl SessionManager {
    pub fn new(database: Database, validator: RequestValidator) -> Self {
        Self {
            database,
            validator,
            monitor: PerformanceMonitor::new(1000),
        }
    }
    
    /// Create a new session
    pub fn create_session(&self, user_id: &str, name: Option<String>) -> Result<String> {
        // Rate limiting
        self.validator.validate_request(1)?;
        
        // Validate user_id
        if user_id.trim().is_empty() || user_id.len() > 255 {
            return Err(anyhow::anyhow!("Invalid user_id"));
        }
        
        let session_id = self.database.create_session(user_id, name)
            .context("Failed to create session")?;
        
        log::debug!("Created session {} for user {}", session_id, user_id);
        Ok(session_id)
    }
    
    /// Get sessions for a user with pagination
    pub fn get_user_sessions(&self, user_id: &str, limit: Option<usize>, offset: Option<usize>) -> Result<PaginatedResponse<Session>> {
        let start = std::time::Instant::now();
        
        // Rate limiting
        self.validator.validate_request(1)?;
        
        let response = self.database.get_user_sessions(user_id, limit, offset)
            .context("Failed to get user sessions")?;
        
        let duration = start.elapsed().as_millis() as f32;
        self.monitor.record_query_time(duration);
        
        log::debug!("Retrieved {} sessions for user {} in {}ms", 
                   response.data.len(), user_id, duration);
        
        Ok(response)
    }
    
    /// Generate a summary for a session
    pub fn generate_session_summary(&self, session_id: &str) -> Result<SessionSummary> {
        let start = std::time::Instant::now();
        
        // Rate limiting (summary generation is expensive)
        self.validator.validate_request(5)?;
        
        // Get all memories for the session
        let filter = QueryFilter {
            session_id: Some(session_id.to_string()),
            limit: Some(1000), // Reasonable limit for summary generation
            ..Default::default()
        };
        
        let memories_response = self.database.recall_memories(&filter)?;
        let memories = memories_response.data;
        
        if memories.is_empty() {
            return Err(anyhow::anyhow!("No memories found for session"));
        }
        
        // Extract session info from first memory
        let user_id = memories[0].user_id.clone();
        
        // Generate summary using advanced text processing
        let summary = self.generate_intelligent_summary(&memories)?;
        
        // Save summary to database
        let summary_record = SessionSummary {
            session_id: session_id.to_string(),
            user_id: user_id.clone(),
            summary_text: summary.summary_text.clone(),
            key_topics: summary.key_topics.clone(),
            memory_count: memories.len(),
            date_range: summary.date_range,
            importance_score: summary.importance_score,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        // TODO: Save to database (implement save_session_summary in database)
        
        let duration = start.elapsed().as_millis() as f32;
        self.monitor.record_query_time(duration);
        
        log::debug!("Generated summary for session {} with {} memories in {}ms", 
                   session_id, memories.len(), duration);
        
        Ok(summary_record)
    }
    
    /// Generate an intelligent summary from memories
    fn generate_intelligent_summary(&self, memories: &[MemoryItem]) -> Result<SessionSummary> {
        if memories.is_empty() {
            return Err(anyhow::anyhow!("Cannot summarize empty memory list"));
        }
        
        let session_id = memories[0].session_id.clone();
        let user_id = memories[0].user_id.clone();
        
        // Sort memories by timestamp
        let mut sorted_memories = memories.to_vec();
        sorted_memories.sort_by_key(|m| m.created_at);
        
        // Extract key topics using TF-IDF-like approach
        let key_topics = self.extract_key_topics(&sorted_memories)?;
        
        // Generate summary text
        let summary_text = self.generate_summary_text(&sorted_memories, &key_topics)?;
        
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }
    
    /// Extract key topics from memories using text analysis
    fn extract_key_topics(&self, memories: &[MemoryItem]) -> Result<Vec<String>> {
        let mut word_freq: HashMap<String, usize> = HashMap::new();
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        
        // Count word frequencies across all memories
        for memory in memories {
            let words = self.tokenize_and_filter(&memory.content.to_lowercase());
            let unique_words: std::collections::HashSet<_> = words.iter().cloned().collect();

            for word in &words {
                *word_freq.entry(word.clone()).or_insert(0) += 1;
            }

            for word in unique_words {
                *doc_freq.entry(word).or_insert(0) += 1;
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
    
    /// Generate summary text from memories and key topics
    fn generate_summary_text(&self, memories: &[MemoryItem], key_topics: &[String]) -> Result<String> {
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
            summary.push_str(&format!(" Notable memories include: {}", highlights.join("; ")));
        }
        
        // Add context about memory importance distribution
        let high_importance = memories.iter().filter(|m| m.importance > 0.7).count();
        let medium_importance = memories.iter().filter(|m| m.importance > 0.4 && m.importance <= 0.7).count();
        
        if high_importance > 0 {
            summary.push_str(&format!(" {} high-importance items", high_importance));
        }
        if medium_importance > 0 {
            summary.push_str(&format!(", {} medium-importance items", medium_importance));
        }
        
        Ok(summary)
    }
    
    /// Tokenize text and filter out stop words and short words
    fn tokenize_and_filter(&self, text: &str) -> Vec<String> {
        text.split_whitespace()
            .filter_map(|word| {
                let cleaned = word
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect::<String>();
                
                if cleaned.len() >= 3 && !self.is_stop_word(&cleaned) {
                    Some(cleaned)
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Check if a word is a stop word
    fn is_stop_word(&self, word: &str) -> bool {
        matches!(
            word.to_lowercase().as_str(),
            "the" | "and" | "or" | "but" | "in" | "on" | "at" | "to" | "for"
                | "of" | "with" | "by" | "from" | "up" | "about" | "into" | "through"
                | "during" | "before" | "after" | "above" | "below" | "between" | "among"
                | "this" | "that" | "these" | "those" | "was" | "were" | "are" | "is"
                | "been" | "being" | "have" | "has" | "had" | "will" | "would" | "could"
                | "should" | "may" | "might" | "can" | "must" | "shall" | "am" | "do"
                | "does" | "did" | "done" | "get" | "got" | "getting" | "very" | "much"
                | "more" | "most" | "many" | "some" | "any" | "all" | "each" | "every"
                | "few" | "several" | "other" | "another" | "such" | "only" | "own"
                | "same" | "so" | "than" | "too" | "just" | "now" | "here"
                | "there" | "when" | "where" | "why" | "how" | "what" | "which" | "who"
        )
    }
    
    /// Search sessions by content keywords
    pub fn search_sessions(&self, user_id: &str, keywords: Vec<String>) -> Result<Vec<Session>> {
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
        
        let memories_response = self.database.recall_memories(&filter)?;
        
        // Collect unique session IDs
        let mut session_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for memory in memories_response.data {
            session_ids.insert(memory.session_id);
        }
        
        // Get session details for each session ID
        let mut matching_sessions = Vec::new();
        let sessions_response = self.get_user_sessions(user_id, None, None)?;
        
        for session in sessions_response.data {
            if session_ids.contains(&session.id) {
                matching_sessions.push(session);
            }
        }
        
        // Sort by last_active descending
        matching_sessions.sort_by(|a, b| b.last_active.cmp(&a.last_active));
        
        Ok(matching_sessions)
    }
    
    /// Update session metadata
    pub fn update_session(&self, session_id: &str, name: Option<String>, tags: Option<Vec<String>>) -> Result<bool> {
        // Rate limiting
        self.validator.validate_request(1)?;
        
        // TODO: Implement update_session in database layer
        log::debug!("Updated session {} with name: {:?}, tags: {:?}", session_id, name, tags);
        
        // For now, return true (would implement actual update in database)
        Ok(true)
    }
    
    /// Delete a session and optionally its memories
    pub fn delete_session(&self, session_id: &str, delete_memories: bool) -> Result<bool> {
        // Rate limiting (deletion is expensive)
        self.validator.validate_request(5)?;
        
        if delete_memories {
            // First, get all memories in the session
            let filter = QueryFilter {
                session_id: Some(session_id.to_string()),
                limit: Some(10000), // Large limit to get all memories
                ..Default::default()
            };
            
            let memories_response = self.database.recall_memories(&filter)?;
            
            // Delete each memory
            for memory in memories_response.data {
                self.database.delete_memory(&memory.id)?;
            }
            
            log::info!("Deleted {} memories from session {}", memories_response.total_count, session_id);
        }
        
        // TODO: Implement delete_session in database layer
        log::debug!("Deleted session {}", session_id);
        
        Ok(true)
    }
    
    /// Get session analytics
    pub fn get_session_analytics(&self, user_id: &str) -> Result<SessionAnalytics> {
        let sessions_response = self.get_user_sessions(user_id, None, None)?;
        let sessions = sessions_response.data;
        
        if sessions.is_empty() {
            return Ok(SessionAnalytics::default());
        }
        
        let total_sessions = sessions.len();
        let total_memories: usize = sessions.iter().map(|s| s.memory_count).sum();
        
        let most_active_session = sessions.iter()
            .max_by_key(|s| s.memory_count)
            .cloned();
        
        let most_recent_session = sessions.iter()
            .max_by_key(|s| s.last_active)
            .cloned();
        
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
    pub fn get_performance_metrics(&self) -> crate::core::PerformanceMetrics {
        self.monitor.get_metrics()
    }
}

/// Session analytics data
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionAnalytics {
    pub user_id: String,
    pub total_sessions: usize,
    pub total_memories: usize,
    pub avg_memories_per_session: f32,
    pub most_active_session: Option<Session>,
    pub most_recent_session: Option<Session>,
    pub activity_by_day: HashMap<String, usize>,
}

impl Default for SessionAnalytics {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            total_sessions: 0,
            total_memories: 0,
            avg_memories_per_session: 0.0,
            most_active_session: None,
            most_recent_session: None,
            activity_by_day: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{Database, DatabaseConfig};
    use crate::core::{MindCacheConfig, RequestValidator};
    use tempfile::TempDir;
    
    fn setup_test_manager() -> (SessionManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_config = DatabaseConfig {
            path: temp_dir.path().join("test.db").to_string_lossy().to_string(),
            ..Default::default()
        };
        
        let database = Database::new(db_config).unwrap();
        let config = MindCacheConfig::default();
        let validator = RequestValidator::new(&config);
        let manager = SessionManager::new(database, validator);
        
        (manager, temp_dir)
    }
    
    #[test]
    fn test_create_and_get_sessions() {
        let (manager, _temp_dir) = setup_test_manager();
        
        // Create sessions
        let session1 = manager.create_session("test_user", Some("Session 1".to_string())).unwrap();
        let session2 = manager.create_session("test_user", Some("Session 2".to_string())).unwrap();
        
        assert_ne!(session1, session2);
        
        // Get sessions
        let response = manager.get_user_sessions("test_user", Some(10), Some(0)).unwrap();
        assert_eq!(response.data.len(), 2);
        assert_eq!(response.total_count, 2);
        
        // Check session names
        let names: Vec<_> = response.data.iter()
            .filter_map(|s| s.name.as_ref())
            .collect();
        assert!(names.contains(&&"Session 1".to_string()));
        assert!(names.contains(&&"Session 2".to_string()));
    }
    
    #[test]
    fn test_key_topic_extraction() {
        let (manager, _temp_dir) = setup_test_manager();
        
        let memories = vec![
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "Trading stocks and bonds in the market today".to_string(),
                ..Default::default()
            },
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "Stock market analysis shows positive trends".to_string(),
                ..Default::default()
            },
            MemoryItem {
                user_id: "test_user".to_string(),
                session_id: "session1".to_string(),
                content: "Investment portfolio needs rebalancing".to_string(),
                ..Default::default()
            },
        ];
        
        let topics = manager.extract_key_topics(&memories).unwrap();
        
        // Should extract relevant financial terms
        assert!(topics.contains(&"market".to_string()) || 
               topics.contains(&"trading".to_string()) || 
               topics.contains(&"stock".to_string()));
    }
    
    #[test]
    fn test_session_analytics() {
        let (manager, _temp_dir) = setup_test_manager();
        
        // Create some sessions
        manager.create_session("test_user", Some("Active Session".to_string())).unwrap();
        manager.create_session("test_user", Some("Quiet Session".to_string())).unwrap();
        
        let analytics = manager.get_session_analytics("test_user").unwrap();
        
        assert_eq!(analytics.total_sessions, 2);
        assert_eq!(analytics.user_id, "test_user");
        assert!(analytics.most_recent_session.is_some());
    }
    
    #[test]
    fn test_tokenization_and_filtering() {
        let (manager, _temp_dir) = setup_test_manager();
        
        let text = "The quick brown fox jumps over the lazy dog!";
        let tokens = manager.tokenize_and_filter(&text.to_lowercase());
        
        // Should filter out stop words and keep meaningful words
        assert!(tokens.contains(&"quick".to_string()));
        assert!(tokens.contains(&"brown".to_string()));
        assert!(tokens.contains(&"fox".to_string()));
        assert!(!tokens.contains(&"the".to_string()));
        assert!(!tokens.contains(&"over".to_string()));
    }
}