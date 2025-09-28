//! Core business logic for MindCache
//!
//! This module contains the main business logic components:
//! - Memory operations and management
//! - Session handling and summaries  
//! - Decay policies and cleanup processes
//! - Async variants for better Node.js integration

pub mod decay;
pub mod memory;
pub mod session;

#[cfg(feature = "async")]
pub mod async_memory;

#[cfg(feature = "async")]
pub mod async_session;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::database::{models::*, Database};

/// Main MindCache configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MindCacheConfig {
    pub database_path: String,

    #[validate(range(min = 1, max = 8760))]
    pub default_memory_ttl_hours: Option<u32>,

    pub auto_decay_enabled: bool,

    #[validate(range(min = 1, max = 168))] // 1 hour to 1 week
    pub decay_interval_hours: u32,

    pub enable_compression: bool,

    #[validate(range(min = 1, max = 1000000))]
    pub max_memories_per_user: usize,

    #[validate(range(min = 0.0, max = 1.0))]
    pub importance_threshold: f32,

    pub enable_request_limits: bool,

    #[validate(range(min = 1, max = 10000))]
    pub max_requests_per_minute: u32,

    #[validate(range(min = 1, max = 1000))]
    pub max_batch_size: usize,
}

impl Default for MindCacheConfig {
    fn default() -> Self {
        Self {
            database_path: "mindcache.db".to_string(),
            default_memory_ttl_hours: Some(24 * 30), // 30 days
            auto_decay_enabled: true,
            decay_interval_hours: 24,
            enable_compression: true,
            max_memories_per_user: 10000,
            importance_threshold: 0.3,
            enable_request_limits: true,
            max_requests_per_minute: 1000,
            max_batch_size: 100,
        }
    }
}

/// Request rate limiter (simple token bucket implementation)
#[derive(Debug)]
pub struct RateLimiter {
    tokens: std::sync::atomic::AtomicU32,
    last_refill: std::sync::Mutex<DateTime<Utc>>,
    max_tokens: u32,
    refill_rate: u32, // tokens per minute
}

impl Clone for RateLimiter {
    fn clone(&self) -> Self {
        Self::new(self.max_tokens, self.refill_rate)
    }
}

impl RateLimiter {
    pub fn new(max_tokens: u32, refill_rate: u32) -> Self {
        Self {
            tokens: std::sync::atomic::AtomicU32::new(max_tokens),
            last_refill: std::sync::Mutex::new(Utc::now()),
            max_tokens,
            refill_rate,
        }
    }

    pub fn try_acquire(&self, tokens: u32) -> bool {
        self.refill_tokens();

        let current_tokens = self.tokens.load(std::sync::atomic::Ordering::Acquire);
        if current_tokens >= tokens {
            let new_tokens = current_tokens - tokens;
            self.tokens
                .store(new_tokens, std::sync::atomic::Ordering::Release);
            true
        } else {
            false
        }
    }

    fn refill_tokens(&self) {
        let now = Utc::now();
        let mut last_refill = self.last_refill.lock().unwrap();

        let duration = now - *last_refill;
        let minutes_elapsed = duration.num_minutes() as u32;

        if minutes_elapsed > 0 {
            let tokens_to_add = minutes_elapsed * (self.refill_rate / 60).max(1);
            let current_tokens = self.tokens.load(std::sync::atomic::Ordering::Acquire);
            let new_tokens = (current_tokens + tokens_to_add).min(self.max_tokens);

            self.tokens
                .store(new_tokens, std::sync::atomic::Ordering::Release);
            *last_refill = now;
        }
    }
}

/// Request validation errors
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Rate limit exceeded. Try again later.")]
    RateLimitExceeded,

    #[error("Batch size too large: {size}. Maximum allowed: {max}")]
    BatchSizeExceeded { size: usize, max: usize },

    #[error("Invalid input: {message}")]
    InvalidInput { message: String },

    #[error("User quota exceeded: {current}/{max}")]
    UserQuotaExceeded { current: usize, max: usize },
}

/// Request validator
#[derive(Clone)]
pub struct RequestValidator {
    rate_limiter: Option<RateLimiter>,
    config: MindCacheConfig,
}

impl RequestValidator {
    pub fn new(config: &MindCacheConfig) -> Self {
        let rate_limiter = if config.enable_request_limits {
            Some(RateLimiter::new(
                config.max_requests_per_minute,
                config.max_requests_per_minute,
            ))
        } else {
            None
        };

        Self {
            rate_limiter,
            config: config.clone(),
        }
    }

    pub fn validate_request(&self, tokens: u32) -> Result<(), ValidationError> {
        if let Some(ref limiter) = self.rate_limiter {
            if !limiter.try_acquire(tokens) {
                return Err(ValidationError::RateLimitExceeded);
            }
        }
        Ok(())
    }

    pub fn validate_batch_size(&self, size: usize) -> Result<(), ValidationError> {
        if size > self.config.max_batch_size {
            return Err(ValidationError::BatchSizeExceeded {
                size,
                max: self.config.max_batch_size,
            });
        }
        Ok(())
    }

    pub fn validate_memory_item(&self, memory: &MemoryItem) -> Result<(), ValidationError> {
        // Use validator crate validation
        memory
            .validate()
            .map_err(|e| ValidationError::InvalidInput {
                message: format!("Memory validation failed: {:?}", e),
            })?;

        // Custom validation
        memory
            .validate_custom()
            .map_err(|e| ValidationError::InvalidInput {
                message: format!("Custom validation failed: {:?}", e),
            })?;

        Ok(())
    }

    pub fn validate_query_filter(&self, filter: &QueryFilter) -> Result<(), ValidationError> {
        filter
            .validate()
            .map_err(|e| ValidationError::InvalidInput {
                message: format!("Filter validation failed: {:?}", e),
            })?;

        Ok(())
    }

    pub async fn validate_user_quota(
        &self,
        db: &Database,
        user_id: &str,
    ) -> Result<(), ValidationError> {
        // Check user's current memory count
        let filter = QueryFilter {
            user_id: Some(user_id.to_string()),
            limit: Some(1), // We just need the count
            ..Default::default()
        };

        let response = db
            .recall_memories(&filter)
            .map_err(|e| ValidationError::InvalidInput {
                message: format!("Database error: {}", e),
            })?;

        if response.total_count as usize >= self.config.max_memories_per_user {
            return Err(ValidationError::UserQuotaExceeded {
                current: response.total_count as usize,
                max: self.config.max_memories_per_user,
            });
        }

        Ok(())
    }
}

/// Batch operations support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest<T> {
    pub items: Vec<T>,
    pub fail_on_error: bool, // If true, entire batch fails on first error
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResponse<T> {
    pub results: Vec<BatchResult<T>>,
    pub success_count: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult<T> {
    pub success: bool,
    pub result: Option<T>,
    pub error: Option<String>,
}

impl<T> BatchResponse<T> {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            success_count: 0,
            error_count: 0,
        }
    }

    pub fn add_success(&mut self, result: T) {
        self.results.push(BatchResult {
            success: true,
            result: Some(result),
            error: None,
        });
        self.success_count += 1;
    }

    pub fn add_error(&mut self, error: String) {
        self.results.push(BatchResult {
            success: false,
            result: None,
            error: Some(error),
        });
        self.error_count += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    pub fn success_rate(&self) -> f32 {
        if self.results.is_empty() {
            0.0
        } else {
            self.success_count as f32 / self.results.len() as f32
        }
    }
}

/// System health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String, // "healthy", "degraded", "unhealthy"
    pub timestamp: DateTime<Utc>,
    pub database_status: String,
    pub memory_usage: MemoryUsage,
    pub performance_metrics: PerformanceMetrics,
    pub recent_errors: Vec<ErrorInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub total_memories: i64,
    pub active_memories: i64,
    pub expired_memories: i64,
    pub database_size_bytes: u64,
    pub memory_growth_rate: f32, // memories per day
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub avg_query_time_ms: f32,
    pub avg_save_time_ms: f32,
    pub queries_per_second: f32,
    pub saves_per_second: f32,
    pub cache_hit_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub timestamp: DateTime<Utc>,
    pub error_type: String,
    pub message: String,
    pub count: u32, // Number of times this error occurred
}

/// Performance monitoring
pub struct PerformanceMonitor {
    query_times: std::sync::Mutex<Vec<f32>>,
    save_times: std::sync::Mutex<Vec<f32>>,
    last_reset: std::sync::Mutex<DateTime<Utc>>,
    max_samples: usize,
}

impl PerformanceMonitor {
    pub fn new(max_samples: usize) -> Self {
        Self {
            query_times: std::sync::Mutex::new(Vec::new()),
            save_times: std::sync::Mutex::new(Vec::new()),
            last_reset: std::sync::Mutex::new(Utc::now()),
            max_samples,
        }
    }

    pub fn record_query_time(&self, duration_ms: f32) {
        let mut times = self.query_times.lock().unwrap();
        times.push(duration_ms);

        if times.len() > self.max_samples {
            times.remove(0);
        }
    }

    pub fn record_save_time(&self, duration_ms: f32) {
        let mut times = self.save_times.lock().unwrap();
        times.push(duration_ms);

        if times.len() > self.max_samples {
            times.remove(0);
        }
    }

    pub fn get_metrics(&self) -> PerformanceMetrics {
        let query_times = self.query_times.lock().unwrap();
        let save_times = self.save_times.lock().unwrap();
        let last_reset = *self.last_reset.lock().unwrap();

        let duration_seconds = (Utc::now() - last_reset).num_seconds() as f32;

        PerformanceMetrics {
            avg_query_time_ms: if query_times.is_empty() {
                0.0
            } else {
                query_times.iter().sum::<f32>() / query_times.len() as f32
            },
            avg_save_time_ms: if save_times.is_empty() {
                0.0
            } else {
                save_times.iter().sum::<f32>() / save_times.len() as f32
            },
            queries_per_second: if duration_seconds > 0.0 {
                query_times.len() as f32 / duration_seconds
            } else {
                0.0
            },
            saves_per_second: if duration_seconds > 0.0 {
                save_times.len() as f32 / duration_seconds
            } else {
                0.0
            },
            cache_hit_rate: 0.0, // TODO: Implement caching
        }
    }

    pub fn reset(&self) {
        self.query_times.lock().unwrap().clear();
        self.save_times.lock().unwrap().clear();
        *self.last_reset.lock().unwrap() = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(10, 60);

        // Should be able to acquire initial tokens
        assert!(limiter.try_acquire(5));
        assert!(limiter.try_acquire(5));

        // Should fail when tokens exhausted
        assert!(!limiter.try_acquire(1));
    }

    #[test]
    fn test_request_validator() {
        let config = MindCacheConfig {
            enable_request_limits: true,
            max_requests_per_minute: 10,
            max_batch_size: 5,
            ..Default::default()
        };

        let validator = RequestValidator::new(&config);

        // Should validate normal request
        assert!(validator.validate_request(1).is_ok());

        // Should validate batch size
        assert!(validator.validate_batch_size(3).is_ok());
        assert!(validator.validate_batch_size(10).is_err());

        // Test memory validation
        let valid_memory = MemoryItem {
            user_id: "test_user".to_string(),
            session_id: "test_session".to_string(),
            content: "Valid content".to_string(),
            importance: 0.5,
            ..Default::default()
        };

        assert!(validator.validate_memory_item(&valid_memory).is_ok());
    }

    #[test]
    fn test_batch_response() {
        let mut response = BatchResponse::<String>::new();

        assert!(response.is_empty());
        assert!(!response.has_errors());
        assert_eq!(response.success_rate(), 0.0);

        response.add_success("result1".to_string());
        response.add_success("result2".to_string());
        response.add_error("error1".to_string());

        assert!(!response.is_empty());
        assert!(response.has_errors());
        assert_eq!(response.success_count, 2);
        assert_eq!(response.error_count, 1);
        assert_eq!(response.success_rate(), 2.0 / 3.0);
    }

    #[test]
    fn test_performance_monitor() {
        let monitor = PerformanceMonitor::new(100);

        // Record some metrics
        monitor.record_query_time(10.0);
        monitor.record_query_time(20.0);
        monitor.record_save_time(5.0);

        let metrics = monitor.get_metrics();
        assert_eq!(metrics.avg_query_time_ms, 15.0);
        assert_eq!(metrics.avg_save_time_ms, 5.0);
        assert!(metrics.queries_per_second > 0.0);
        assert!(metrics.saves_per_second > 0.0);

        // Test reset
        monitor.reset();
        let metrics_after_reset = monitor.get_metrics();
        assert_eq!(metrics_after_reset.avg_query_time_ms, 0.0);
    }

    #[test]
    fn test_config_validation() {
        let mut config = MindCacheConfig::default();
        assert!(config.validate().is_ok());

        // Test invalid values
        config.default_memory_ttl_hours = Some(0);
        assert!(config.validate().is_err());

        config.default_memory_ttl_hours = Some(24);
        config.importance_threshold = 1.5;
        assert!(config.validate().is_err());
    }
}
