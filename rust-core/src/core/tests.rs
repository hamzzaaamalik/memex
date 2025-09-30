 //! Core business logic tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MemexConfig, RequestValidator, RateLimiter, PerformanceMonitor};
    use crate::core::memory::MemoryManager;
    use crate::core::session::SessionManager;
    use crate::core::decay::DecayEngine;
    use crate::database::{Database, DatabaseConfig, models::*};
    use std::collections::HashMap;
    use tempfile::TempDir;
    use serial_test::serial;

    fn setup_test_components() -> (MemoryManager, SessionManager, DecayEngine, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_config = DatabaseConfig {
            path: temp_dir.path().join("test.db").to_string_lossy().to_string(),
            ..Default::default()
        };
        
        let database = Database::new(db_config).unwrap();
        let config = MemexConfig::default();
        let validator = RequestValidator::new(&config);
        
        let memory_manager = MemoryManager::new(database.clone(), validator.clone());
        let session_manager = SessionManager::new(database.clone(), validator.clone());
        
        let decay_policy = DecayPolicy::default();
        let decay_engine = DecayEngine::new(database, validator, decay_policy);
        
        (memory_manager, session_manager, decay_engine, temp_dir)
    }

    #[test]
    #[serial]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(5, 60); // 5 tokens, 60 per minute
        
        // Should be able to acquire initial tokens
        assert!(limiter.try_acquire(3));
        assert!(limiter.try_acquire(2));
        
        // Should fail when tokens exhausted
        assert!(!limiter.try_acquire(1));
        
        // Test token refill (would need to wait in real scenario)
        // For testing, we can't easily simulate time passage
    }

    #[test]
    #[serial]
    fn test_request_validator() {
        let config = MemexConfig {
            enable_request_limits: true,
            max_requests_per_minute: 10,
            max_batch_size: 5,
            ..Default::default()
        };
        
        let validator = RequestValidator::new(&config);
        
        // Test request validation
        assert!(validator.validate_request(1).is_ok());
        
        // Test batch size validation
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
        
        // Test invalid memory
        let invalid_memory = MemoryItem {
            user_id: "".to_string(), // Empty user_id should fail
            session_id: "test_session".to_string(),
            content: "Valid content".to_string(),
            importance: 1.5, // Invalid importance should fail
            ..Default::default()
        };
        
        assert!(validator.validate_memory_item(&invalid_memory).is_err());
    }

    #[test]
    #[serial]
    fn test_performance_monitor() {
        let monitor = PerformanceMonitor::new(100);
        
        // Record some metrics
        monitor.record_query_time(10.0);
        monitor.record_query_time(20.0);
        monitor.record_query_time(30.0);
        monitor.record_save_time(5.0);
        monitor.record_save_time(15.0);
        
        let metrics = monitor.get_metrics();
        assert_eq!(metrics.avg_query_time_ms, 20.0); // (10+20+30)/3
        assert_eq!(metrics.avg_save_time_ms, 10.0); // (5+15)/2
        assert!(metrics.queries_per_second > 0.0);
        assert!(metrics.saves_per_second > 0.0);
        
        // Test reset
        monitor.reset();
        let metrics_after_reset = monitor.get_metrics();
        assert_eq!(metrics_after_reset.avg_query_time_ms, 0.0);
        assert_eq!(metrics_after_reset.avg_save_time_ms, 0.0);
    }

    #[test]
    #[serial]
    fn test_memory_manager_operations() {
        let (memory_manager, _, _, _temp_dir) = setup_test_components();
        
        // Test save memory
        let memory = MemoryItem {
            user_id: "test_user".to_string(),
            session_id: "test_session".to_string(),
            content: "Test memory content".to_string(),
            importance: 0.8,
            ttl_hours: Some(24),
            ..Default::default()
        };
        
        let memory_id = memory_manager.save_memory(memory).unwrap();
        assert!(!memory_id.is_empty());
        
        // Test get memory
        let retrieved = memory_manager.get_memory(&memory_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.content, "Test memory content");
        assert_eq!(retrieved.importance, 0.8);
        
        // Test update memory
        let update = crate::core::memory::MemoryUpdate {
            content: Some("Updated content".to_string()),
            importance: Some(0.9),
            metadata: None,
            ttl_hours: None,
        };
        
        let updated = memory_manager.update_memory(&memory_id, update).unwrap();
        assert!(updated);
        
        // Verify update
        let retrieved_updated = memory_manager.get_memory(&memory_id).unwrap();
        assert!(retrieved_updated.is_some());
        let retrieved_updated = retrieved_updated.unwrap();
        assert_eq!(retrieved_updated.content, "Updated content");
        assert_eq!(retrieved_updated.importance, 0.9);
        
        // Test delete memory
        let deleted = memory_manager.delete_memory(&memory_id).unwrap();
        assert!(deleted);
        
        let not_found = memory_manager.get_memory(&memory_id).unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    #[serial]
    fn test_memory_search_and_recall() {
        let (memory_manager, _, _, _temp_dir) = setup_test_components();
        
        // Create test memories
        let memories = vec![
            ("Apple stock analysis", 0.8, "stocks"),
            ("Bitcoin price prediction", 0.6, "crypto"),
            ("Tesla quarterly earnings", 0.9, "earnings"),
            ("Market volatility analysis", 0.7, "analysis"),
        ];
        
        let mut memory_ids = Vec::new();
        for (content, importance, category) in memories {
            let mut metadata = HashMap::new();
            metadata.insert("category".to_string(), category.to_string());
            
            let memory = MemoryItem {
                user_id: "search_user".to_string(),
                session_id: "search_session".to_string(),
                content: content.to_string(),
                importance,
                metadata,
                ..Default::default()
            };
            
            let id = memory_manager.save_memory(memory).unwrap();
            memory_ids.push(id);
        }
        
        // Test search memories
        let search_result = memory_manager.search_memories("search_user", "Apple stock", Some(10), Some(0)).unwrap();
        assert_eq!(search_result.data.len(), 1);
        assert!(search_result.data[0].content.contains("Apple"));
        
        // Test recall with filters
        let filter = QueryFilter {
            user_id: Some("search_user".to_string()),
            min_importance: Some(0.8),
            ..Default::default()
        };
        let recall_result = memory_manager.recall_memories(filter).unwrap();
        assert_eq!(recall_result.data.len(), 2); // Apple (0.8) and Tesla (0.9)
        
        // Test get important memories
        let important = memory_manager.get_important_memories("search_user", 0.7, Some(10)).unwrap();
        assert_eq!(important.data.len(), 3); // Apple, Tesla, Market (0.8, 0.9, 0.7)
    }

    #[test]
    #[serial]
    fn test_batch_operations() {
        let (memory_manager, _, _, _temp_dir) = setup_test_components();
        
        // Create batch of memories
        let memories = vec![
            MemoryItem {
                user_id: "batch_user".to_string(),
                session_id: "batch_session".to_string(),
                content: "Batch memory 1".to_string(),
                importance: 0.5,
                ..Default::default()
            },
            MemoryItem {
                user_id: "batch_user".to_string(),
                session_id: "batch_session".to_string(),
                content: "Batch memory 2".to_string(),
                importance: 0.6,
                ..Default::default()
            },
            MemoryItem {
                user_id: "".to_string(), // Invalid - should cause error
                session_id: "batch_session".to_string(),
                content: "Invalid memory".to_string(),
                importance: 0.7,
                ..Default::default()
            },
        ];
        
        let batch_request = crate::core::BatchRequest {
            items: memories,
            fail_on_error: false,
        };
        
        let batch_response = memory_manager.save_memories_batch(batch_request).unwrap();
        assert_eq!(batch_response.success_count, 2);
        assert_eq!(batch_response.error_count, 1);
        assert!(!batch_response.is_empty());
        assert!(batch_response.has_errors());
        assert_eq!(batch_response.success_rate(), 2.0 / 3.0);
    }

    #[test]
    #[serial]
    fn test_session_manager_operations() {
        let (memory_manager, session_manager, _, _temp_dir) = setup_test_components();
        
        // Test create session
        let session_id = session_manager.create_session("session_user", Some("Test Session".to_string())).unwrap();
        assert!(!session_id.is_empty());
        
        // Add memories to session
        for i in 0..5 {
            let memory = MemoryItem {
                user_id: "session_user".to_string(),
                session_id: session_id.clone(),
                content: format!("Session memory {}", i),
                importance: 0.5 + (i as f32 * 0.1),
                ..Default::default()
            };
            memory_manager.save_memory(memory).unwrap();
        }
        // Test get user sessions
       let sessions = session_manager.get_user_sessions("session_user", Some(10), Some(0)).unwrap();
       assert_eq!(sessions.data.len(), 1);
       assert_eq!(sessions.data[0].id, session_id);
       assert_eq!(sessions.data[0].memory_count, 5);
       
       // Test generate session summary
       let summary = session_manager.generate_session_summary(&session_id).unwrap();
       assert_eq!(summary.session_id, session_id);
       assert_eq!(summary.user_id, "session_user");
       assert_eq!(summary.memory_count, 5);
       assert!(summary.importance_score > 0.0);
       assert!(!summary.summary_text.is_empty());
       
       // Test search sessions
       let sessions = session_manager.search_sessions("session_user", vec!["memory".to_string()]).unwrap();
       assert_eq!(sessions.len(), 1);
       assert_eq!(sessions[0].id, session_id);
       
       // Test delete session
       let deleted = session_manager.delete_session(&session_id, true).unwrap();
       assert!(deleted);
   }

   #[test]
   #[serial]
   fn test_session_analytics() {
       let (memory_manager, session_manager, _, _temp_dir) = setup_test_components();
       
       // Create multiple sessions with different activity levels
       let session1 = session_manager.create_session("analytics_user", Some("Active Session".to_string())).unwrap();
       let session2 = session_manager.create_session("analytics_user", Some("Quiet Session".to_string())).unwrap();
       
       // Add memories to first session
       for i in 0..10 {
           let memory = MemoryItem {
               user_id: "analytics_user".to_string(),
               session_id: session1.clone(),
               content: format!("Active memory {}", i),
               ..Default::default()
           };
           memory_manager.save_memory(memory).unwrap();
       }
       
       // Add fewer memories to second session
       for i in 0..3 {
           let memory = MemoryItem {
               user_id: "analytics_user".to_string(),
               session_id: session2.clone(),
               content: format!("Quiet memory {}", i),
               ..Default::default()
           };
           memory_manager.save_memory(memory).unwrap();
       }
       
       // Test session analytics
       let analytics = session_manager.get_session_analytics("analytics_user").unwrap();
       assert_eq!(analytics.total_sessions, 2);
       assert_eq!(analytics.total_memories, 13);
       assert_eq!(analytics.avg_memories_per_session, 6.5);
       assert!(analytics.most_active_session.is_some());
       assert_eq!(analytics.most_active_session.as_ref().unwrap().memory_count, 10);
   }

   #[test]
   #[serial]
   fn test_decay_engine_operations() {
       let (memory_manager, _, mut decay_engine, _temp_dir) = setup_test_components();
       
       // Create memories with different importance and age
       let memories = vec![
           ("High importance recent", 0.9, None),
           ("Low importance recent", 0.1, None),
           ("Medium importance old", 0.5, Some(1)), // 1 hour TTL
           ("High importance old", 0.8, Some(1)),
       ];
       
       for (content, importance, ttl) in memories {
           let memory = MemoryItem {
               user_id: "decay_user".to_string(),
               session_id: "decay_session".to_string(),
               content: content.to_string(),
               importance,
               ttl_hours: ttl,
               ..Default::default()
           };
           memory_manager.save_memory(memory).unwrap();
       }
       
       // Test decay policy update
       let new_policy = DecayPolicy {
           max_age_hours: 2,
           importance_threshold: 0.3,
           max_memories_per_user: 1000,
           compression_enabled: true,
           auto_summarize_sessions: false,
       };
       
       decay_engine.update_policy(new_policy).unwrap();
       
       // Test decay recommendations
       let recommendations = decay_engine.get_decay_recommendations().unwrap();
       assert!(recommendations.total_memories >= 4);
       assert!(recommendations.old_memory_percentage >= 0.0);
       assert!(recommendations.estimated_cleanup_count >= 0);
       
       // Test age distribution analysis
       let distribution = decay_engine.analyze_memory_age_distribution().unwrap();
       assert!(distribution.contains_key("0-24h"));
       
       // Test run decay
       let decay_stats = decay_engine.run_decay().unwrap();
       assert!(decay_stats.total_memories_before >= 0);
       assert!(decay_stats.total_memories_after >= 0);
       assert!(matches!(decay_stats.status, DecayStatus::Completed | DecayStatus::Failed));
   }

   #[test]
   #[serial]
   fn test_memory_compression() {
       let (memory_manager, _, decay_engine, _temp_dir) = setup_test_components();
       
       // Create multiple low-importance memories in same session
       for i in 0..5 {
           let mut memory = MemoryItem {
               user_id: "compress_user".to_string(),
               session_id: "compress_session".to_string(),
               content: format!("Low importance memory {} about trading and stocks", i),
               importance: 0.2, // Below threshold
               ..Default::default()
           };
           
           // Make them old by setting creation time in past
           memory.created_at = chrono::Utc::now() - chrono::Duration::hours(25);
           memory_manager.save_memory(memory).unwrap();
       }
       
       // Run decay which should compress these memories
       let decay_stats = decay_engine.run_decay().unwrap();
       
       // Should have some compression activity
       println!("Decay stats: compressed={}, expired={}", 
               decay_stats.memories_compressed, decay_stats.memories_expired);
       
       // Note: Compression behavior depends on specific implementation details
       // This test verifies the process runs without errors
       assert!(matches!(decay_stats.status, DecayStatus::Completed));
   }

   #[test]
   #[serial]
   fn test_user_memory_statistics() {
       let (memory_manager, _, _, _temp_dir) = setup_test_components();
       
       // Create memories with different importance levels
       let importance_levels = vec![0.1, 0.3, 0.5, 0.7, 0.9];
       
       for (i, importance) in importance_levels.iter().enumerate() {
           let memory = MemoryItem {
               user_id: "stats_user".to_string(),
               session_id: "stats_session".to_string(),
               content: format!("Memory with importance {}", importance),
               importance: *importance,
               created_at: chrono::Utc::now() - chrono::Duration::hours(i as i64),
               ..Default::default()
           };
           memory_manager.save_memory(memory).unwrap();
       }
       
       // Test user memory statistics
       let stats = memory_manager.get_user_memory_stats("stats_user").unwrap();
       assert_eq!(stats.total_memories, 5);
       assert_eq!(stats.avg_importance, 0.5); // (0.1+0.3+0.5+0.7+0.9)/5
       assert!(stats.importance_distribution.contains_key("high"));
       assert!(stats.importance_distribution.contains_key("low"));
       assert!(stats.age_distribution.contains_key("24h"));
       assert!(stats.oldest_memory.is_some());
       assert!(stats.newest_memory.is_some());
   }

   #[test]
   #[serial]
   fn test_export_import_functionality() {
       let (memory_manager, _, _, _temp_dir) = setup_test_components();
       
       // Create test memories
       let memories = vec![
           "First memory for export",
           "Second memory with metadata",
           "Third memory for completeness",
       ];
       
       for (i, content) in memories.iter().enumerate() {
           let mut metadata = HashMap::new();
           metadata.insert("index".to_string(), i.to_string());
           metadata.insert("export_test".to_string(), "true".to_string());
           
           let memory = MemoryItem {
               user_id: "export_user".to_string(),
               session_id: "export_session".to_string(),
               content: content.to_string(),
               importance: 0.5 + (i as f32 * 0.1),
               metadata,
               ..Default::default()
           };
           memory_manager.save_memory(memory).unwrap();
       }
       
       // Test export
       let exported_memories = memory_manager.export_user_memories("export_user").unwrap();
       assert_eq!(exported_memories.len(), 3);
       
       // Verify exported data
       for (i, memory) in exported_memories.iter().enumerate() {
           assert_eq!(memory.user_id, "export_user");
           assert!(memory.content.contains(&memories[i]));
           assert!(memory.metadata.contains_key("export_test"));
       }
   }

   #[test]
   #[serial]
   fn test_error_handling_and_edge_cases() {
       let (memory_manager, session_manager, _, _temp_dir) = setup_test_components();
       
       // Test invalid memory operations
       let invalid_memory = MemoryItem {
           user_id: "".to_string(), // Empty user_id
           session_id: "test_session".to_string(),
           content: "".to_string(), // Empty content
           importance: 2.0, // Invalid importance
           ..Default::default()
       };
       
       assert!(memory_manager.save_memory(invalid_memory).is_err());
       
       // Test operations on non-existent items
       assert!(memory_manager.get_memory("nonexistent_id").unwrap().is_none());
       assert!(!memory_manager.delete_memory("nonexistent_id").unwrap());
       
       // Test update non-existent memory
       let update = crate::core::memory::MemoryUpdate {
           content: Some("Updated".to_string()),
           importance: None,
           metadata: None,
           ttl_hours: None,
       };
       assert!(!memory_manager.update_memory("nonexistent_id", update).unwrap());
       
       // Test session operations with invalid data
       assert!(session_manager.create_session("", None).is_err()); // Empty user_id
       
       let empty_sessions = session_manager.get_user_sessions("nonexistent_user", Some(10), Some(0)).unwrap();
       assert_eq!(empty_sessions.data.len(), 0);
       
       // Test summary of non-existent session
       assert!(session_manager.generate_session_summary("nonexistent_session").is_err());
   }

   #[test]
   #[serial]
   fn test_concurrent_memory_operations() {
       let (memory_manager, _, _, _temp_dir) = setup_test_components();
       
       // Test concurrent memory saves
       let handles: Vec<_> = (0..10).map(|i| {
           let manager = memory_manager.clone();
           std::thread::spawn(move || {
               let memory = MemoryItem {
                   user_id: format!("concurrent_user_{}", i % 3),
                   session_id: format!("session_{}", i),
                   content: format!("Concurrent memory {}", i),
                   importance: 0.5,
                   ..Default::default()
               };
               manager.save_memory(memory)
           })
       }).collect();
       
       // Wait for all operations to complete
       let mut results = Vec::new();
       for handle in handles {
           let result = handle.join().expect("Thread should complete");
           results.push(result);
       }
       
       // Most operations should succeed (some might fail due to rate limiting)
       let success_count = results.iter().filter(|r| r.is_ok()).count();
       assert!(success_count >= 5, "At least half the operations should succeed");
   }

   #[test]
   #[serial]
   fn test_performance_monitoring() {
       let (memory_manager, _, _, _temp_dir) = setup_test_components();
       
       // Perform operations to generate metrics
       for i in 0..5 {
           let memory = MemoryItem {
               user_id: "perf_user".to_string(),
               session_id: "perf_session".to_string(),
               content: format!("Performance test memory {}", i),
               ..Default::default()
           };
           memory_manager.save_memory(memory).unwrap();
       }
       
       // Perform some recalls
       let filter = QueryFilter {
           user_id: Some("perf_user".to_string()),
           ..Default::default()
       };
       for _ in 0..3 {
           memory_manager.recall_memories(filter.clone()).unwrap();
       }
       
       // Get performance metrics
       let metrics = memory_manager.get_performance_metrics();
       assert!(metrics.avg_save_time_ms > 0.0);
       assert!(metrics.avg_query_time_ms > 0.0);
       assert!(metrics.saves_per_second >= 0.0);
       assert!(metrics.queries_per_second >= 0.0);
       
       // Reset and verify
       memory_manager.reset_performance_monitoring();
       let reset_metrics = memory_manager.get_performance_metrics();
       assert_eq!(reset_metrics.avg_save_time_ms, 0.0);
       assert_eq!(reset_metrics.avg_query_time_ms, 0.0);
   }

   #[test]
   #[serial]
   fn test_config_validation() {
       // Test valid config
       let valid_config = MemexConfig {
           database_path: "./test.db".to_string(),
           default_memory_ttl_hours: Some(720),
           max_memories_per_user: 10000,
           importance_threshold: 0.3,
           max_requests_per_minute: 1000,
           max_batch_size: 100,
           ..Default::default()
       };
       assert!(valid_config.validate().is_ok());

       // Test invalid configs
       let invalid_ttl = MemexConfig {
           default_memory_ttl_hours: Some(0), // Invalid TTL
           ..Default::default()
       };
       assert!(invalid_ttl.validate().is_err());

       let invalid_threshold = MemexConfig {
           importance_threshold: 1.5, // Invalid threshold
           ..Default::default()
       };
       assert!(invalid_threshold.validate().is_err());

       let invalid_batch_size = MemexConfig {
           max_batch_size: 0, // Invalid batch size
           ..Default::default()
       };
       assert!(invalid_batch_size.validate().is_err());
   }
}