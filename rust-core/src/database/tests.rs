//! Database layer tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{Database, DatabaseConfig, models::*};
    use chrono::{DateTime, Utc};
    use std::collections::HashMap;
    use tempfile::TempDir;
    use serial_test::serial;

    fn create_test_database() -> (Database, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = DatabaseConfig {
            path: temp_dir.path().join("test.db").to_string_lossy().to_string(),
            enable_wal: true,
            cache_size: -2000, // 2MB for tests
            busy_timeout: 5000,
            synchronous: "NORMAL".to_string(),
        };
        
        let database = Database::new(config).expect("Failed to create database");
        (database, temp_dir)
    }

    fn create_test_memory(user_id: &str, session_id: &str, content: &str) -> MemoryItem {
        MemoryItem {
            id: String::new(), // Will be generated
            user_id: user_id.to_string(),
            session_id: session_id.to_string(),
            content: content.to_string(),
            importance: 0.5,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            ..Default::default()
        }
    }

    #[test]
    #[serial]
    fn test_database_initialization() {
        let (database, _temp_dir) = create_test_database();
        
        // Database should be created and accessible
        let stats = database.get_stats().expect("Should get stats");
        assert!(stats.is_object());
        
        // Should have expected tables (verify by trying operations)
        let filter = QueryFilter::default();
        let result = database.recall_memories(&filter).expect("Should query memories");
        assert_eq!(result.data.len(), 0);
        assert_eq!(result.total_count, 0);
    }

    #[test]
    #[serial]
    fn test_memory_crud_operations() {
        let (database, _temp_dir) = create_test_database();
        
        // Test Create
        let memory = create_test_memory("test_user", "test_session", "Test memory content");
        let memory_id = database.save_memory(&memory).expect("Should save memory");
        assert!(!memory_id.is_empty());
        
        // Test Read
        let retrieved = database.get_memory(&memory_id).expect("Should get memory");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.user_id, "test_user");
        assert_eq!(retrieved.content, "Test memory content");
        assert_eq!(retrieved.importance, 0.5);
        
        // Test Update (save with same ID)
        let mut updated_memory = retrieved.clone();
        updated_memory.content = "Updated content".to_string();
        updated_memory.importance = 0.8;
        
        let updated_id = database.save_memory(&updated_memory).expect("Should update memory");
        assert_eq!(updated_id, memory_id);
        
        let retrieved_updated = database.get_memory(&memory_id).expect("Should get updated memory");
        assert!(retrieved_updated.is_some());
        let retrieved_updated = retrieved_updated.unwrap();
        assert_eq!(retrieved_updated.content, "Updated content");
        assert_eq!(retrieved_updated.importance, 0.8);
        
        // Test Delete
        let deleted = database.delete_memory(&memory_id).expect("Should delete memory");
        assert!(deleted);
        
        let not_found = database.get_memory(&memory_id).expect("Should handle missing memory");
        assert!(not_found.is_none());
    }

    #[test]
    #[serial]
    fn test_memory_recall_filtering() {
        let (database, _temp_dir) = create_test_database();
        
        // Create test memories
        let memories = vec![
            ("user1", "session1", "Apple stock is performing well", 0.8),
            ("user1", "session1", "Bitcoin price is volatile", 0.6),
            ("user1", "session2", "Tesla earnings beat expectations", 0.9),
            ("user2", "session3", "Market analysis for Q4", 0.7),
            ("user2", "session3", "Portfolio rebalancing needed", 0.4),
        ];
        
        let mut memory_ids = Vec::new();
        for (user, session, content, importance) in memories {
            let mut memory = create_test_memory(user, session, content);
            memory.importance = importance;
            let id = database.save_memory(&memory).expect("Should save memory");
            memory_ids.push(id);
        }
        
        // Test user filtering
        let filter = QueryFilter {
            user_id: Some("user1".to_string()),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should recall user1 memories");
        assert_eq!(result.data.len(), 3);
        assert_eq!(result.total_count, 3);
        
        // Test session filtering
        let filter = QueryFilter {
            user_id: Some("user1".to_string()),
            session_id: Some("session1".to_string()),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should recall session1 memories");
        assert_eq!(result.data.len(), 2);
        
        // Test importance filtering
        let filter = QueryFilter {
            min_importance: Some(0.7),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should recall high importance memories");
        assert_eq!(result.data.len(), 3); // 0.8, 0.9, 0.7
        
        // Test limit and offset
        let filter = QueryFilter {
            limit: Some(2),
            offset: Some(1),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should recall with pagination");
        assert_eq!(result.data.len(), 2);
        assert_eq!(result.page, 0); // offset 1 with limit 2 = page 0
        assert_eq!(result.per_page, 2);
        assert!(result.total_count >= 2);
    }

    #[test]
    #[serial]
    fn test_full_text_search() {
        let (database, _temp_dir) = create_test_database();
        
        // Create memories with searchable content
        let memories = vec![
            "Apple stock trading at all-time high",
            "Bitcoin cryptocurrency market analysis",
            "Tesla electric vehicle sales growth", 
            "Microsoft cloud computing revenue",
            "Amazon logistics and delivery optimization",
        ];
        
        for content in memories {
            let memory = create_test_memory("search_user", "search_session", content);
            database.save_memory(&memory).expect("Should save memory");
        }
        
        // Test keyword search
        let filter = QueryFilter {
            user_id: Some("search_user".to_string()),
            keywords: Some(vec!["stock".to_string()]),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should find stock memories");
        assert_eq!(result.data.len(), 1);
        assert!(result.data[0].content.contains("Apple"));
        
        // Test multiple keywords (OR search)
        let filter = QueryFilter {
            user_id: Some("search_user".to_string()),
            keywords: Some(vec!["Bitcoin".to_string(), "Tesla".to_string()]),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should find Bitcoin OR Tesla");
        assert_eq!(result.data.len(), 2);
        
        // Test partial word matching
        let filter = QueryFilter {
            user_id: Some("search_user".to_string()),
            keywords: Some(vec!["comput".to_string()]), // Should match "computing"
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should find computing");
        assert_eq!(result.data.len(), 1);
        assert!(result.data[0].content.contains("Microsoft"));
    }

    #[test]
    #[serial]
    fn test_memory_expiration() {
        let (database, _temp_dir) = create_test_database();
        
        // Create memory with TTL
        let mut memory = create_test_memory("ttl_user", "ttl_session", "Expiring memory");
        memory.ttl_hours = Some(1); // 1 hour TTL
        
        let memory_id = database.save_memory(&memory).expect("Should save memory with TTL");
        
        // Memory should exist
        let retrieved = database.get_memory(&memory_id).expect("Should get memory");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert!(retrieved.expires_at.is_some());
        
        // Simulate expiration by manually setting expires_at to past
        let mut expired_memory = retrieved;
        expired_memory.expires_at = Some(Utc::now() - chrono::Duration::hours(2));
        database.save_memory(&expired_memory).expect("Should update expiry");
        
        // Test cleanup of expired memories
        let cleaned = database.cleanup_expired().expect("Should cleanup expired");
        assert_eq!(cleaned, 1);
        
        // Memory should no longer be retrievable
        let not_found = database.get_memory(&memory_id).expect("Should handle expired memory");
        assert!(not_found.is_none());
    }

    #[test]
    #[serial]
    fn test_session_operations() {
        let (database, _temp_dir) = create_test_database();
        
        // Create session
        let session_id = database.create_session("session_user", Some("Test Session".to_string()))
            .expect("Should create session");
        assert!(!session_id.is_empty());
        
        // Add memories to session
        for i in 0..5 {
            let memory = create_test_memory("session_user", &session_id, &format!("Memory {}", i));
            database.save_memory(&memory).expect("Should save session memory");
        }
        
        // Get user sessions
        let sessions = database.get_user_sessions("session_user", Some(10), Some(0))
            .expect("Should get user sessions");
        assert_eq!(sessions.data.len(), 1);
        assert_eq!(sessions.data[0].id, session_id);
        assert_eq!(sessions.data[0].memory_count, 5);
        assert_eq!(sessions.data[0].name, Some("Test Session".to_string()));
    }

    #[test]
    #[serial]
    fn test_database_statistics() {
        let (database, _temp_dir) = create_test_database();
        
        // Add test data
        let users = ["stats_user1", "stats_user2"];
        for user in &users {
            for i in 0..3 {
                let memory = create_test_memory(user, &format!("{}_session", user), &format!("Memory {}", i));
                database.save_memory(&memory).expect("Should save memory");
            }
        }
        
        // Get stats
        let stats = database.get_stats().expect("Should get statistics");
        
        // Verify structure
        assert!(stats.get("total_memories").is_some());
        assert!(stats.get("user_counts").is_some());
        assert!(stats.get("database_size_bytes").is_some());
        
        // Verify user counts
        let user_counts = stats.get("user_counts").unwrap();
        assert!(user_counts.get("stats_user1").is_some());
        assert!(user_counts.get("stats_user2").is_some());
    }

    #[test]
    #[serial]
    fn test_concurrent_access() {
        let (database, _temp_dir) = create_test_database();
        
        // Simulate concurrent writes
        let handles: Vec<_> = (0..10).map(|i| {
            let db = database.clone();
            std::thread::spawn(move || {
                let memory = create_test_memory(
                    &format!("concurrent_user_{}", i % 3),
                    &format!("session_{}", i),
                    &format!("Concurrent memory {}", i)
                );
                db.save_memory(&memory)
            })
        }).collect();
        
        // Wait for all threads
        let mut results = Vec::new();
        for handle in handles {
            let result = handle.join().expect("Thread should complete");
            results.push(result);
        }
        
        // All operations should succeed
        assert_eq!(results.len(), 10);
        for result in results {
            assert!(result.is_ok());
        }
        
        // Verify all memories were saved
        let filter = QueryFilter::default();
        let all_memories = database.recall_memories(&filter).expect("Should recall all memories");
        assert_eq!(all_memories.total_count, 10);
    }

    #[test]
    #[serial]
    fn test_large_content_handling() {
        let (database, _temp_dir) = create_test_database();
        
        // Test with large content (100KB)
        let large_content = "A".repeat(100_000);
        let memory = create_test_memory("large_user", "large_session", &large_content);
        
        let memory_id = database.save_memory(&memory).expect("Should save large content");
        
        // Retrieve and verify
        let retrieved = database.get_memory(&memory_id).expect("Should get large memory");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.content.len(), 100_000);
        assert_eq!(retrieved.content, large_content);
    }

    #[test]
    #[serial]
    fn test_unicode_content() {
        let (database, _temp_dir) = create_test_database();
        
        // Test with Unicode content
        let unicode_content = "Hello 世界 🌍 café naïve résumé Москва العالم";
        let memory = create_test_memory("unicode_user", "unicode_session", unicode_content);
        
        let memory_id = database.save_memory(&memory).expect("Should save Unicode content");
        
        // Retrieve and verify
        let retrieved = database.get_memory(&memory_id).expect("Should get Unicode memory");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.content, unicode_content);
        
        // Test Unicode search
        let filter = QueryFilter {
            keywords: Some(vec!["世界".to_string()]),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should search Unicode");
        assert_eq!(result.data.len(), 1);
    }

    #[test]
    #[serial]
    fn test_metadata_handling() {
        let (database, _temp_dir) = create_test_database();
        
        // Create memory with complex metadata
        let mut metadata = HashMap::new();
        metadata.insert("category".to_string(), "trading".to_string());
        metadata.insert("asset".to_string(), "AAPL".to_string());
        metadata.insert("confidence".to_string(), "0.85".to_string());
        metadata.insert("tags".to_string(), "tech,growth,dividend".to_string());
        
        let mut memory = create_test_memory("meta_user", "meta_session", "Apple stock analysis");
        memory.metadata = metadata.clone();
        
        let memory_id = database.save_memory(&memory).expect("Should save memory with metadata");
        
        // Retrieve and verify metadata
        let retrieved = database.get_memory(&memory_id).expect("Should get memory");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.metadata, metadata);
        assert_eq!(retrieved.metadata.get("category"), Some(&"trading".to_string()));
        assert_eq!(retrieved.metadata.get("asset"), Some(&"AAPL".to_string()));
    }

    #[test]
    #[serial]
    fn test_database_resilience() {
        let (database, _temp_dir) = create_test_database();
        
        // Test invalid memory ID
        let not_found = database.get_memory("invalid_id").expect("Should handle invalid ID");
        assert!(not_found.is_none());
        
        // Test delete non-existent memory
        let not_deleted = database.delete_memory("invalid_id").expect("Should handle invalid delete");
        assert!(!not_deleted);
        
        // Test empty filter
        let filter = QueryFilter::default();
        let result = database.recall_memories(&filter).expect("Should handle empty filter");
        assert_eq!(result.data.len(), 0);
        
        // Test invalid user session query
        let sessions = database.get_user_sessions("nonexistent_user", Some(10), Some(0))
            .expect("Should handle nonexistent user");
        assert_eq!(sessions.data.len(), 0);
        assert_eq!(sessions.total_count, 0);
    }

    #[test]
    #[serial]
    fn test_pagination_edge_cases() {
        let (database, _temp_dir) = create_test_database();
        
        // Create test memories
        for i in 0..25 {
            let memory = create_test_memory("page_user", "page_session", &format!("Memory {}", i));
            database.save_memory(&memory).expect("Should save memory");
        }
        
        // Test first page
        let filter = QueryFilter {
            user_id: Some("page_user".to_string()),
            limit: Some(10),
            offset: Some(0),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should get first page");
        assert_eq!(result.data.len(), 10);
        assert_eq!(result.page, 0);
        assert_eq!(result.per_page, 10);
        assert_eq!(result.total_count, 25);
        assert_eq!(result.total_pages, 3);
        assert!(result.has_next);
        assert!(!result.has_prev);
        
        // Test middle page
        let filter = QueryFilter {
            user_id: Some("page_user".to_string()),
            limit: Some(10),
            offset: Some(10),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should get middle page");
        assert_eq!(result.data.len(), 10);
        assert_eq!(result.page, 1);
        assert!(result.has_next);
        assert!(result.has_prev);
        
        // Test last page
        let filter = QueryFilter {
            user_id: Some("page_user".to_string()),
            limit: Some(10),
            offset: Some(20),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should get last page");
        assert_eq!(result.data.len(), 5); // Remaining memories
        assert_eq!(result.page, 2);
        assert!(!result.has_next);
        assert!(result.has_prev);
        
        // Test beyond last page
        let filter = QueryFilter {
            user_id: Some("page_user".to_string()),
            limit: Some(10),
            offset: Some(100),
            ..Default::default()
        };
        let result = database.recall_memories(&filter).expect("Should handle beyond last page");
        assert_eq!(result.data.len(), 0);
    }
}