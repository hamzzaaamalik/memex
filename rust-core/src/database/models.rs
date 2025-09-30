//! Data models for Memex database

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::{Validate, ValidationError};

/// A memory item stored in the database with vector support
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemoryItem {
    pub id: String,

    #[validate(length(min = 1, max = 255))]
    pub user_id: String,

    #[validate(length(min = 1, max = 255))]
    pub session_id: String,

    #[validate(length(min = 1, max = 1000000))] // 1MB max content
    pub content: String,

    pub content_vector: Option<String>, // For future vector embeddings

    // Vector embedding fields (only available with vector-search feature)
    #[cfg(feature = "vector-search")]
    pub embedding: Option<Vec<f32>>, // Vector embedding
    #[cfg(feature = "vector-search")]
    pub embedding_model: Option<String>, // Model used for embedding

    #[serde(default)]
    pub metadata: HashMap<String, String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,

    #[validate(range(min = 0.0, max = 1.0))]
    pub importance: f32,

    #[validate(range(min = 1, max = 8760))] // Max 1 year TTL
    pub ttl_hours: Option<u32>,

    pub is_compressed: bool,

    #[serde(default)]
    pub compressed_from: Vec<String>, // IDs of original memories if this is compressed
}

impl Default for MemoryItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            user_id: String::new(),
            session_id: String::new(),
            content: String::new(),
            content_vector: None,
            #[cfg(feature = "vector-search")]
            embedding: None,
            #[cfg(feature = "vector-search")]
            embedding_model: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            importance: 0.5,
            ttl_hours: None,
            is_compressed: false,
            compressed_from: Vec::new(),
        }
    }
}

/// Query filter for searching memories
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct QueryFilter {
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,

    #[validate(range(min = 1, max = 1000))]
    pub limit: Option<usize>,

    #[validate(range(min = 0, max = 1000000))]
    pub offset: Option<usize>,

    #[validate(range(min = 0.0, max = 1.0))]
    pub min_importance: Option<f32>,
}

impl Default for QueryFilter {
    fn default() -> Self {
        Self {
            user_id: None,
            session_id: None,
            keywords: None,
            date_from: None,
            date_to: None,
            limit: Some(50), // Default page size
            offset: Some(0),
            min_importance: None,
        }
    }
}

/// Paginated response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total_count: i64,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
    pub has_next: bool,
    pub has_prev: bool,
}

impl<T> PaginatedResponse<T> {
    pub fn empty() -> Self {
        Self {
            data: Vec::new(),
            total_count: 0,
            page: 0,
            per_page: 50,
            total_pages: 0,
            has_next: false,
            has_prev: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

/// Session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub memory_count: usize,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
}

/// Session summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub user_id: String,
    pub summary_text: String,
    pub key_topics: Vec<String>,
    pub memory_count: usize,
    pub date_range: (DateTime<Utc>, DateTime<Utc>),
    pub importance_score: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Compressed memory metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedMemory {
    pub id: String,
    pub original_ids: Vec<String>,
    pub user_id: String,
    pub session_id: String,
    pub summary: String,
    pub key_points: Vec<String>,
    pub date_range: (DateTime<Utc>, DateTime<Utc>),
    pub original_count: usize,
    pub combined_importance: f32,
    pub compressed_at: DateTime<Utc>,
}

/// Decay policy configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DecayPolicy {
    #[validate(range(min = 1, max = 8760))] // 1 hour to 1 year
    pub max_age_hours: u32,

    #[validate(range(min = 0.0, max = 1.0))]
    pub importance_threshold: f32,

    #[validate(range(min = 1, max = 1000000))]
    pub max_memories_per_user: usize,

    pub compression_enabled: bool,
    pub auto_summarize_sessions: bool,
}

impl Default for DecayPolicy {
    fn default() -> Self {
        Self {
            max_age_hours: 24 * 30, // 30 days
            importance_threshold: 0.3,
            max_memories_per_user: 10000,
            compression_enabled: true,
            auto_summarize_sessions: true,
        }
    }
}

/// Decay process statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayStats {
    pub run_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub memories_expired: usize,
    pub memories_compressed: usize,
    pub sessions_summarized: usize,
    pub total_memories_before: usize,
    pub total_memories_after: usize,
    pub storage_saved_bytes: usize,
    pub status: DecayStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DecayStatus {
    Running,
    Completed,
    Failed,
}

impl std::fmt::Display for DecayStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecayStatus::Running => write!(f, "running"),
            DecayStatus::Completed => write!(f, "completed"),
            DecayStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for DecayStatus {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "running" => Ok(DecayStatus::Running),
            "completed" => Ok(DecayStatus::Completed),
            "failed" => Ok(DecayStatus::Failed),
            _ => Err("Invalid decay status"),
        }
    }
}

/// Database statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub total_memories: i64,
    pub active_memories: i64,
    pub expired_memories: i64,
    pub compressed_memories: i64,
    pub total_users: i64,
    pub total_sessions: i64,
    pub database_size_bytes: u64,
    pub memory_by_importance: HashMap<String, i64>,
    pub memory_by_age: HashMap<String, i64>,
    pub top_users: Vec<UserStats>,
    pub recent_activity: Vec<ActivityStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStats {
    pub user_id: String,
    pub memory_count: i64,
    pub session_count: i64,
    pub last_active: DateTime<Utc>,
    pub avg_importance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityStats {
    pub date: String, // YYYY-MM-DD format
    pub memory_count: i64,
    pub unique_users: i64,
    pub avg_importance: f32,
}

/// Validation helpers
impl MemoryItem {
    /// Custom validation beyond derive macro
    pub fn validate_custom(&self) -> Result<(), ValidationError> {
        // Check content is not just whitespace
        if self.content.trim().is_empty() {
            return Err(ValidationError::new("content_empty"));
        }

        // Check metadata size (prevent abuse)
        let metadata_size: usize = self.metadata.iter().map(|(k, v)| k.len() + v.len()).sum();

        if metadata_size > 10000 {
            // 10KB max metadata
            return Err(ValidationError::new("metadata_too_large"));
        }

        // Validate compressed_from consistency
        if self.is_compressed && self.compressed_from.is_empty() {
            return Err(ValidationError::new("compressed_without_originals"));
        }

        if !self.is_compressed && !self.compressed_from.is_empty() {
            return Err(ValidationError::new("not_compressed_with_originals"));
        }

        Ok(())
    }
}

impl QueryFilter {
    /// Create a simple filter for user + keywords
    pub fn for_user_with_keywords(user_id: &str, keywords: Vec<String>) -> Self {
        Self {
            user_id: Some(user_id.to_string()),
            keywords: if keywords.is_empty() {
                None
            } else {
                Some(keywords)
            },
            ..Default::default()
        }
    }

    /// Create a filter for a specific session
    pub fn for_session(user_id: &str, session_id: &str) -> Self {
        Self {
            user_id: Some(user_id.to_string()),
            session_id: Some(session_id.to_string()),
            ..Default::default()
        }
    }

    /// Create a filter for high-importance memories
    pub fn high_importance(user_id: &str, threshold: f32) -> Self {
        Self {
            user_id: Some(user_id.to_string()),
            min_importance: Some(threshold),
            ..Default::default()
        }
    }

    /// Create a filter with date range
    pub fn date_range(user_id: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        Self {
            user_id: Some(user_id.to_string()),
            date_from: Some(from),
            date_to: Some(to),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_memory_item_validation() {
        let mut memory = MemoryItem {
            user_id: "test_user".to_string(),
            session_id: "test_session".to_string(),
            content: "Valid content".to_string(),
            ..Default::default()
        };

        // Should validate successfully
        assert!(memory.validate().is_ok());
        assert!(memory.validate_custom().is_ok());

        // Test empty content
        memory.content = "   ".to_string();
        assert!(memory.validate_custom().is_err());

        // Test invalid importance
        memory.content = "Valid content".to_string();
        memory.importance = 1.5;
        assert!(memory.validate().is_err());

        // Test compressed without originals
        memory.importance = 0.8;
        memory.is_compressed = true;
        assert!(memory.validate_custom().is_err());

        // Fix compressed memory
        memory.compressed_from = vec!["mem1".to_string(), "mem2".to_string()];
        assert!(memory.validate_custom().is_ok());
    }

    #[test]
    fn test_query_filter_helpers() {
        let filter = QueryFilter::for_user_with_keywords(
            "user123",
            vec!["trading".to_string(), "stocks".to_string()],
        );

        assert_eq!(filter.user_id, Some("user123".to_string()));
        assert_eq!(
            filter.keywords,
            Some(vec!["trading".to_string(), "stocks".to_string()])
        );

        let session_filter = QueryFilter::for_session("user123", "session456");
        assert_eq!(session_filter.user_id, Some("user123".to_string()));
        assert_eq!(session_filter.session_id, Some("session456".to_string()));

        let importance_filter = QueryFilter::high_importance("user123", 0.8);
        assert_eq!(importance_filter.min_importance, Some(0.8));
    }

    #[test]
    fn test_paginated_response() {
        let response = PaginatedResponse::<String>::empty();
        assert!(response.is_empty());
        assert_eq!(response.len(), 0);
        assert_eq!(response.total_count, 0);

        let response = PaginatedResponse {
            data: vec!["item1".to_string(), "item2".to_string()],
            total_count: 10,
            page: 0,
            per_page: 2,
            total_pages: 5,
            has_next: true,
            has_prev: false,
        };

        assert!(!response.is_empty());
        assert_eq!(response.len(), 2);
        assert!(response.has_next);
        assert!(!response.has_prev);
    }

    #[test]
    fn test_decay_status() {
        assert_eq!(DecayStatus::Running.to_string(), "running");
        assert_eq!(DecayStatus::Completed.to_string(), "completed");
        assert_eq!(DecayStatus::Failed.to_string(), "failed");

        assert_eq!(
            "running".parse::<DecayStatus>().unwrap().to_string(),
            "running"
        );
        assert_eq!(
            "COMPLETED".parse::<DecayStatus>().unwrap().to_string(),
            "completed"
        );
        assert!("invalid".parse::<DecayStatus>().is_err());
    }
}
