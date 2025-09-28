//! Integration tests for MindCache
//! 
//! These tests verify that all components work together correctly
//! and test realistic usage scenarios.

use mindcache_core::*;
use mindcache_core::database::{Database, DatabaseConfig};
use mindcache_core::core::{MindCacheConfig, RequestValidator};
use mindcache_core::core::memory::MemoryManager;
use mindcache_core::core::session::SessionManager;
use mindcache_core::core::decay::DecayEngine;
use mindcache_core::database::models::*;
use std::collections::HashMap;
use tempfile::TempDir;
use serial_test::serial;

struct TestEnvironment {
    memory_manager: MemoryManager,
    session_manager: SessionManager,
    decay_engine: DecayEngine,
    config: MindCacheConfig,
    _temp_dir: TempDir,
}

impl TestEnvironment {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = MindCacheConfig {
            database_path: temp_dir.path().join("integration_test.db").to_string_lossy().to_string(),
            auto_decay_enabled: false, // Disable for predictable testing
            decay_interval_hours: 24,
            default_memory_ttl_hours: Some(24),
            enable_compression: true,
            max_memories_per_user: 1000,
            importance_threshold: 0.3,
            enable_request_limits: false, // Disable for testing
            max_requests_per_minute: 1000,
            max_batch_size: 100,
        };
        
        let db_config = DatabaseConfig {
            path: config.database_path.clone(),
            ..Default::default()
        };
        
        let database = Database::new(db_config).expect("Failed to create database");
        let validator = RequestValidator::new(&config);
        
        let memory_manager = MemoryManager::new(database.clone(), validator.clone());
        let session_manager = SessionManager::new(database.clone(), validator.clone());
        
        let decay_policy = DecayPolicy {
            max_age_hours: config.default_memory_ttl_hours.unwrap_or(24),
            importance_threshold: config.importance_threshold,
            max_memories_per_user: config.max_memories_per_user,
            compression_enabled: config.enable_compression,
            auto_summarize_sessions: true,
        };
        
        let decay_engine = DecayEngine::new(database, validator, decay_policy);
        
        Self {
            memory_manager,
            session_manager,
            decay_engine,
            config,
            _temp_dir: temp_dir,
        }
    }
}

#[test]
#[serial]
fn test_full_memory_lifecycle() {
    let env = TestEnvironment::new();
    
    // Create a session
    let session_id = env.session_manager
        .create_session("test_user", Some("Test Session".to_string()))
        .expect("Should create session");
    
    // Save memory
    let memory = MemoryItem {
        user_id: "test_user".to_string(),
        session_id: session_id.clone(),
        content: "Test memory content".to_string(),
        importance: 0.8,
        ttl_hours: Some(48),
        metadata: {
            let mut meta = HashMap::new();
            meta.insert("category".to_string(), "test".to_string());
            meta
        },
        ..Default::default()
    };
    
    let memory_id = env.memory_manager.save_memory(memory).expect("Should save memory");
    assert!(!memory_id.is_empty());
    
    // Recall memory
    let filter = QueryFilter {
        user_id: Some("test_user".to_string()),
        keywords: Some(vec!["Test".to_string()]),
        ..Default::default()
    };
    
    let memories = env.memory_manager.recall_memories(filter).expect("Should recall memories");
    assert_eq!(memories.data.len(), 1);
    assert_eq!(memories.data[0].content, "Test memory content");
    assert_eq!(memories.data[0].user_id, "test_user");
    assert_eq!(memories.data[0].session_id, session_id);
    assert_eq!(memories.data[0].importance, 0.8);
    
    // Update memory
    let update = crate::core::memory::MemoryUpdate {
        content: Some("Updated test content".to_string()),
        importance: Some(0.9),
        metadata: None,
        ttl_hours: None,
    };
    
    let updated = env.memory_manager.update_memory(&memory_id, update).expect("Should update memory");
    assert!(updated);
    
    // Verify update
    let updated_memory = env.memory_manager.get_memory(&memory_id).expect("Should get updated memory");
    assert!(updated_memory.is_some());
    let updated_memory = updated_memory.unwrap();
    assert_eq!(updated_memory.content, "Updated test content");
    assert_eq!(updated_memory.importance, 0.9);
    
    // Delete memory
    let deleted = env.memory_manager.delete_memory(&memory_id).expect("Should delete memory");
    assert!(deleted);
    
    let not_found = env.memory_manager.get_memory(&memory_id).expect("Should handle deleted memory");
    assert!(not_found.is_none());
}

#[test]
#[serial]
fn test_multi_user_scenario() {
    let env = TestEnvironment::new();
    
    // Create sessions for different users
    let alice_session = env.session_manager
        .create_session("alice", Some("Alice's Trading Journal".to_string()))
        .expect("Should create Alice session");
    let bob_session = env.session_manager
        .create_session("bob", Some("Bob's Investment Notes".to_string()))
        .expect("Should create Bob session");
    
    // Alice's memories
    let alice_memories = vec![
        ("Bought AAPL at $175. Strong technical indicators.", 0.8),
        ("Set stop loss for AAPL at $170. Risk management is key.", 0.9),
        ("Tesla earnings tomorrow. Expecting volatility.", 0.7),
    ];
    
    for (content, importance) in alice_memories {
        let memory = MemoryItem {
            user_id: "alice".to_string(),
            session_id: alice_session.clone(),
            content: content.to_string(),
            importance,
            ..Default::default()
        };
        env.memory_manager.save_memory(memory).expect("Should save Alice's memory");
    }
    
    // Bob's memories
    let bob_memories = vec![
        ("Portfolio review: too heavy in tech stocks.", 0.8),
        ("Consider adding defensive stocks for balance.", 0.6),
        ("Research real estate investment trusts (REITs).", 0.5),
    ];
    
    for (content, importance) in bob_memories {
        let memory = MemoryItem {
            user_id: "bob".to_string(),
            session_id: bob_session.clone(),
            content: content.to_string(),
            importance,
            ..Default::default()
        };
        env.memory_manager.save_memory(memory).expect("Should save Bob's memory");
    }
    
    // Verify isolation - Alice can't see Bob's memories
    let alice_memories = env.memory_manager.recall_memories(QueryFilter {
        user_id: Some("alice".to_string()),
        ..Default::default()
    }).expect("Should recall Alice's memories");
    assert_eq!(alice_memories.data.len(), 3);
    assert!(alice_memories.data.iter().all(|m| m.user_id == "alice"));
    assert!(alice_memories.data.iter().any(|m| m.content.contains("AAPL")));
    
    let bob_memories = env.memory_manager.recall_memories(QueryFilter {
        user_id: Some("bob".to_string()),
        ..Default::default()
    }).expect("Should recall Bob's memories");
    assert_eq!(bob_memories.data.len(), 3);
    assert!(bob_memories.data.iter().all(|m| m.user_id == "bob"));
    assert!(bob_memories.data.iter().any(|m| m.content.contains("Portfolio")));
    
    // Test cross-user search (should return nothing)
    let cross_search = env.memory_manager.search_memories("alice", "Portfolio", Some(10), Some(0))
        .expect("Should handle cross-user search");
    assert_eq!(cross_search.data.len(), 0);
}

#[test]
#[serial]
fn test_session_management_workflow() {
    let env = TestEnvironment::new();
    
    let user_id = "session_test_user";
    
    // Create multiple sessions with different themes
    let trading_session = env.session_manager
        .create_session(user_id, Some("Day Trading".to_string()))
        .expect("Should create trading session");
    let research_session = env.session_manager
        .create_session(user_id, Some("Market Research".to_string()))
        .expect("Should create research session");
    let planning_session = env.session_manager
        .create_session(user_id, Some("Investment Planning".to_string()))
        .expect("Should create planning session");
    
    // Add memories to each session
    let sessions_data = vec![
        (&trading_session, vec![
            "Opened TSLA position at $240",
            "TSLA hit resistance at $250, taking partial profits",
            "Closed TSLA position at $248 for +3.3% gain",
        ]),
        (&research_session, vec![
            "Fed meeting minutes suggest dovish stance",
            "Semiconductor sector showing signs of recovery",
            "Consumer spending data mixed, retail under pressure",
        ]),
        (&planning_session, vec![
            "Rebalance portfolio: reduce tech exposure to 60%",
            "Increase cash position to 15% for opportunities",
            "Research international diversification options",
        ]),
    ];
    
    for (session_id, contents) in sessions_data {
        for content in contents {
            let memory = MemoryItem {
                user_id: user_id.to_string(),
                session_id: session_id.clone(),
                content: content.to_string(),
                importance: 0.6,
                ..Default::default()
            };
            env.memory_manager.save_memory(memory).expect("Should save session memory");
        }
    }
    
    // Test get user sessions
    let sessions = env.session_manager
        .get_user_sessions(user_id, Some(10), Some(0))
        .expect("Should get user sessions");
    assert_eq!(sessions.data.len(), 3);
    assert_eq!(sessions.total_count, 3);
    
    // Verify session memory counts
    for session in &sessions.data {
        assert_eq!(session.memory_count, 3);
        assert!(session.name.is_some());
    }
    
    // Test session summaries
    for session in &sessions.data {
        let summary = env.session_manager
            .generate_session_summary(&session.id)
            .expect("Should generate session summary");
        
        assert_eq!(summary.session_id, session.id);
        assert_eq!(summary.user_id, user_id);
        assert_eq!(summary.memory_count, 3);
        assert!(!summary.summary_text.is_empty());
        assert!(!summary.key_topics.is_empty());
        assert!(summary.importance_score > 0.0);
    }
    
    // Test search sessions
    let trading_sessions = env.session_manager
        .search_sessions(user_id, vec!["TSLA".to_string()])
        .expect("Should find trading sessions");
    assert_eq!(trading_sessions.len(), 1);
    assert_eq!(trading_sessions[0].id, trading_session);
    
    let fed_sessions = env.session_manager
        .search_sessions(user_id, vec!["Fed".to_string()])
        .expect("Should find research sessions");
    assert_eq!(fed_sessions.len(), 1);
    assert_eq!(fed_sessions[0].id, research_session);
}

#[test]
#[serial]
fn test_advanced_search_and_filtering() {
    let env = TestEnvironment::new();
    
    let user_id = "advanced_search_user";
    let session_id = env.session_manager
        .create_session(user_id, Some("Advanced Search Test".to_string()))
        .expect("Should create session");
    
    // Create memories with various attributes for testing
    let test_data = vec![
        ("Apple earnings beat expectations, stock up 5%", 0.9, "tech", "AAPL"),
        ("Microsoft cloud revenue grows 30% year-over-year", 0.8, "tech", "MSFT"), 
        ("Oil prices surge on supply concerns", 0.7, "commodities", "OIL"),
        ("Fed keeps rates unchanged, markets rally", 0.8, "macro", "FED"),
        ("Bitcoin breaks above $50k resistance level", 0.6, "crypto", "BTC"),
        ("Tesla delivery numbers disappoint analysts", 0.5, "tech", "TSLA"),
        ("Gold hits new yearly high on inflation fears", 0.6, "commodities", "GOLD"),
        ("Housing market shows signs of cooling", 0.4, "real-estate", "HOUSING"),
    ];
    
    for (content, importance, category, symbol) in test_data {
        let mut metadata = HashMap::new();
        metadata.insert("category".to_string(), category.to_string());
        metadata.insert("symbol".to_string(), symbol.to_string());
        
        let memory = MemoryItem {
            user_id: user_id.to_string(),
            session_id: session_id.clone(),
            content: content.to_string(),
            importance,
            metadata,
            created_at: chrono::Utc::now() - chrono::Duration::hours(rand::random::<u32>() as i64 % 48),
            ..Default::default()
        };
        env.memory_manager.save_memory(memory).expect("Should save test memory");
    }
    
    // Test full-text search
    let search_results = env.memory_manager
        .search_memories(user_id, "Apple earnings", Some(10), Some(0))
        .expect("Should search memories");
    assert_eq!(search_results.data.len(), 1);
    assert!(search_results.data[0].content.contains("Apple"));
    
    // Test importance filtering
    let high_importance = env.memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        min_importance: Some(0.8),
        ..Default::default()
    }).expect("Should filter by importance");
    assert_eq!(high_importance.data.len(), 3); // Apple, Microsoft, Fed
    
    // Test keyword filtering
    let tech_memories = env.memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        keywords: Some(vec!["tech".to_string(), "stock".to_string()]),
        ..Default::default()
    }).expect("Should filter by keywords");
    assert!(tech_memories.data.len() >= 2);
    
    // Test pagination
    let first_page = env.memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        limit: Some(3),
        offset: Some(0),
        ..Default::default()
    }).expect("Should get first page");
    assert_eq!(first_page.data.len(), 3);
    assert_eq!(first_page.page, 0);
    assert!(first_page.has_next);
    assert!(!first_page.has_prev);
    
    let second_page = env.memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        limit: Some(3),
        offset: Some(3),
        ..Default::default()
    }).expect("Should get second page");
    assert_eq!(second_page.data.len(), 3);
    assert_eq!(second_page.page, 1);
    assert!(second_page.has_next);
    assert!(second_page.has_prev);
    
    // Test date range filtering
    let recent_memories = env.memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        date_from: Some(chrono::Utc::now() - chrono::Duration::hours(24)),
       date_to: Some(chrono::Utc::now()),
       ..Default::default()
   }).expect("Should filter by date range");
   assert!(recent_memories.data.len() > 0);
   
   // Test combined filters
   let combined_filter = env.memory_manager.recall_memories(QueryFilter {
       user_id: Some(user_id.to_string()),
       keywords: Some(vec!["tech".to_string()]),
       min_importance: Some(0.7),
       limit: Some(5),
       ..Default::default()
   }).expect("Should apply combined filters");
   
   // Should find tech memories with importance >= 0.7
   assert!(combined_filter.data.len() >= 1);
   for memory in &combined_filter.data {
       assert!(memory.importance >= 0.7);
       assert!(memory.content.to_lowercase().contains("tech") || 
              memory.content.contains("Apple") || 
              memory.content.contains("Microsoft") ||
              memory.content.contains("Tesla"));
   }
}

#[test]
#[serial]
fn test_batch_operations_workflow() {
   let env = TestEnvironment::new();
   
   let user_id = "batch_user";
   let session_id = env.session_manager
       .create_session(user_id, Some("Batch Test Session".to_string()))
       .expect("Should create session");
   
   // Create a batch of memories with mixed validity
   let batch_memories = vec![
       MemoryItem {
           user_id: user_id.to_string(),
           session_id: session_id.clone(),
           content: "Valid batch memory 1".to_string(),
           importance: 0.6,
           ..Default::default()
       },
       MemoryItem {
           user_id: user_id.to_string(),
           session_id: session_id.clone(),
           content: "Valid batch memory 2".to_string(),
           importance: 0.7,
           ..Default::default()
       },
       MemoryItem {
           user_id: user_id.to_string(),
           session_id: session_id.clone(),
           content: "Valid batch memory 3".to_string(),
           importance: 0.8,
           ..Default::default()
       },
       MemoryItem {
           user_id: "".to_string(), // Invalid - empty user_id
           session_id: session_id.clone(),
           content: "Invalid batch memory".to_string(),
           importance: 0.5,
           ..Default::default()
       },
       MemoryItem {
           user_id: user_id.to_string(),
           session_id: session_id.clone(),
           content: "Another valid memory".to_string(),
           importance: 2.0, // Invalid - importance > 1.0
           ..Default::default()
       },
   ];
   
   // Test batch save with fail_on_error = false
   let batch_request = crate::core::BatchRequest {
       items: batch_memories.clone(),
       fail_on_error: false,
   };
   
   let batch_response = env.memory_manager
       .save_memories_batch(batch_request)
       .expect("Should complete batch save");
   
   assert_eq!(batch_response.results.len(), 5);
   assert_eq!(batch_response.success_count, 3); // 3 valid memories
   assert_eq!(batch_response.error_count, 2); // 2 invalid memories
   assert!(batch_response.has_errors());
   assert_eq!(batch_response.success_rate(), 0.6);
   
   // Verify successful memories were saved
   let saved_memories = env.memory_manager.recall_memories(QueryFilter {
       user_id: Some(user_id.to_string()),
       ..Default::default()
   }).expect("Should recall batch memories");
   assert_eq!(saved_memories.data.len(), 3);
   
   // Test batch save with fail_on_error = true
   let fail_fast_request = crate::core::BatchRequest {
       items: batch_memories,
       fail_on_error: true,
   };
   
   let fail_fast_response = env.memory_manager
       .save_memories_batch(fail_fast_request)
       .expect("Should handle fail-fast batch");
   
   // Should stop on first error
   assert!(fail_fast_response.error_count > 0);
   assert!(fail_fast_response.success_count < 5);
}

#[test]
#[serial]
fn test_memory_decay_integration() {
   let env = TestEnvironment::new();
   
   let user_id = "decay_integration_user";
   let session_id = env.session_manager
       .create_session(user_id, Some("Decay Integration Test".to_string()))
       .expect("Should create session");
   
   // Create memories with different characteristics for decay testing
   let decay_test_memories = vec![
       // High importance, should survive
       ("Critical trading alert: Market crash imminent", 0.95, Some(48), false),
       ("Emergency: Stop all trading immediately", 0.9, Some(72), false),
       
       // Medium importance, may survive
       ("Market volatility increasing, be cautious", 0.6, Some(24), false),
       ("Portfolio review scheduled for next week", 0.5, Some(48), false),
       
       // Low importance, likely to be removed
       ("Coffee shop was busy today", 0.1, Some(1), true), // Should expire soon
       ("Weather is nice, good for market sentiment", 0.2, Some(2), true),
       ("Random thought about diversification", 0.3, Some(6), false),
       
       // Old memories for compression testing
       ("Old trading note from last month", 0.4, None, false),
       ("Previous market analysis", 0.35, None, false),
       ("Historical price observation", 0.3, None, false),
   ];
   
   let mut memory_ids = Vec::new();
   for (content, importance, ttl, make_old) in decay_test_memories {
       let mut memory = MemoryItem {
           user_id: user_id.to_string(),
           session_id: session_id.clone(),
           content: content.to_string(),
           importance,
           ttl_hours: ttl,
           ..Default::default()
       };
       
       // Make some memories old for testing
       if make_old {
           memory.created_at = chrono::Utc::now() - chrono::Duration::hours(25);
       }
       
       let id = env.memory_manager.save_memory(memory).expect("Should save decay test memory");
       memory_ids.push(id);
   }
   
   // Get initial count
   let initial_memories = env.memory_manager.recall_memories(QueryFilter {
       user_id: Some(user_id.to_string()),
       ..Default::default()
   }).expect("Should get initial memories");
   let initial_count = initial_memories.data.len();
   
   // Test decay analysis
   let recommendations = env.decay_engine
       .get_decay_recommendations()
       .expect("Should get decay recommendations");
   
   assert!(recommendations.total_memories >= initial_count);
   assert!(recommendations.old_memory_percentage >= 0.0);
   assert!(recommendations.estimated_cleanup_count >= 0);
   
   // Test age distribution
   let age_distribution = env.decay_engine
       .analyze_memory_age_distribution()
       .expect("Should analyze age distribution");
   
   assert!(age_distribution.contains_key("0-24h"));
   let total_in_distribution: usize = age_distribution.values().sum();
   assert!(total_in_distribution >= initial_count);
   
   // Run decay process
   let decay_stats = env.decay_engine.run_decay().expect("Should run decay");
   
   assert!(decay_stats.total_memories_before >= initial_count);
   assert!(decay_stats.total_memories_after <= decay_stats.total_memories_before);
   assert!(matches!(decay_stats.status, DecayStatus::Completed));
   
   // Verify high-importance memories survived
   let surviving_memories = env.memory_manager.recall_memories(QueryFilter {
       user_id: Some(user_id.to_string()),
       min_importance: Some(0.8),
       ..Default::default()
   }).expect("Should get surviving high-importance memories");
   
   assert!(surviving_memories.data.len() >= 2); // At least the 2 high-importance ones
   assert!(surviving_memories.data.iter().any(|m| m.content.contains("Critical")));
   assert!(surviving_memories.data.iter().any(|m| m.content.contains("Emergency")));
   
   println!("Decay results: expired={}, compressed={}, before={}, after={}", 
            decay_stats.memories_expired,
            decay_stats.memories_compressed,
            decay_stats.total_memories_before,
            decay_stats.total_memories_after);
}

#[test]
#[serial]
fn test_statistics_and_analytics() {
   let env = TestEnvironment::new();
   
   // Create test data across multiple users and sessions
   let users = vec!["analytics_user1", "analytics_user2", "analytics_user3"];
   let mut all_session_ids = Vec::new();
   
   for (user_index, user) in users.iter().enumerate() {
       // Create multiple sessions per user
       for session_index in 0..3 {
           let session_name = format!("Session {} for {}", session_index + 1, user);
           let session_id = env.session_manager
               .create_session(user, Some(session_name))
               .expect("Should create session");
           all_session_ids.push((user, session_id.clone()));
           
           // Add different numbers of memories per session
           let memory_count = (user_index + 1) * (session_index + 1) * 2;
           for i in 0..memory_count {
               let memory = MemoryItem {
                   user_id: user.to_string(),
                   session_id: session_id.clone(),
                   content: format!("Memory {} for {} in session {}", i, user, session_index),
                   importance: 0.3 + (i as f32 % 7) * 0.1, // Varying importance 0.3-0.9
                   created_at: chrono::Utc::now() - chrono::Duration::hours(i as i64 % 48),
                   ..Default::default()
               };
               env.memory_manager.save_memory(memory).expect("Should save analytics memory");
           }
       }
   }
   
   // Test user memory statistics
   for user in &users {
       let user_stats = env.memory_manager
           .get_user_memory_stats(user)
           .expect("Should get user stats");
       
       assert!(user_stats.total_memories > 0);
       assert!(user_stats.avg_importance >= 0.0 && user_stats.avg_importance <= 1.0);
       assert!(!user_stats.importance_distribution.is_empty());
       assert!(!user_stats.age_distribution.is_empty());
       assert!(user_stats.oldest_memory.is_some());
       assert!(user_stats.newest_memory.is_some());
       
       // Verify importance distribution categories
       let total_dist: i32 = user_stats.importance_distribution.values().sum();
       assert_eq!(total_dist as i64, user_stats.total_memories);
   }
   
   // Test session analytics
   for user in &users {
       let session_analytics = env.session_manager
           .get_session_analytics(user)
           .expect("Should get session analytics");
       
       assert_eq!(session_analytics.total_sessions, 3);
       assert!(session_analytics.total_memories > 0);
       assert!(session_analytics.avg_memories_per_session > 0.0);
       assert!(session_analytics.most_active_session.is_some());
       assert!(session_analytics.most_recent_session.is_some());
       
       // Verify most active session has the highest memory count
       let most_active = session_analytics.most_active_session.unwrap();
       let user_sessions = env.session_manager
           .get_user_sessions(user, Some(10), Some(0))
           .expect("Should get user sessions");
       
       let max_memory_count = user_sessions.data.iter()
           .map(|s| s.memory_count)
           .max()
           .unwrap_or(0);
       assert_eq!(most_active.memory_count, max_memory_count);
   }
   
   // Test export functionality
   for user in &users {
       let exported_data = env.memory_manager
           .export_user_memories(user)
           .expect("Should export user memories");
       
       assert!(!exported_data.is_empty());
       
       // Parse exported JSON to verify structure
       let parsed: Vec<MemoryItem> = serde_json::from_str(&exported_data)
           .expect("Exported data should be valid JSON");
       
       assert!(parsed.len() > 0);
       assert!(parsed.iter().all(|m| m.user_id == *user));
       
       // Verify essential fields are present
       for memory in &parsed {
           assert!(!memory.id.is_empty());
           assert!(!memory.content.is_empty());
           assert!(memory.importance >= 0.0 && memory.importance <= 1.0);
       }
   }
}

#[test]
#[serial]
fn test_performance_under_load() {
   let env = TestEnvironment::new();
   
   let user_id = "performance_user";
   let session_id = env.session_manager
       .create_session(user_id, Some("Performance Test Session".to_string()))
       .expect("Should create session");
   
   // Test save performance
   let save_start = std::time::Instant::now();
   let num_saves = 1000;
   
   for i in 0..num_saves {
       let memory = MemoryItem {
           user_id: user_id.to_string(),
           session_id: session_id.clone(),
           content: format!("Performance test memory {} with some meaningful content", i),
           importance: 0.5 + (i % 10) as f32 * 0.05,
           ..Default::default()
       };
       env.memory_manager.save_memory(memory).expect("Should save performance memory");
   }
   
   let save_duration = save_start.elapsed();
   let saves_per_second = num_saves as f64 / save_duration.as_secs_f64();
   
   println!("Save performance: {} saves in {:?} ({:.2} saves/sec)", 
            num_saves, save_duration, saves_per_second);
   
   // Should achieve reasonable performance (adjust threshold as needed)
   assert!(saves_per_second > 50.0, "Save performance too slow: {:.2} saves/sec", saves_per_second);
   
   // Test recall performance
   let recall_start = std::time::Instant::now();
   let num_recalls = 100;
   
   for i in 0..num_recalls {
       let filter = QueryFilter {
           user_id: Some(user_id.to_string()),
           keywords: Some(vec![format!("{}", i % 10)]),
           limit: Some(50),
           ..Default::default()
       };
       env.memory_manager.recall_memories(filter).expect("Should recall memories");
   }
   
   let recall_duration = recall_start.elapsed();
   let recalls_per_second = num_recalls as f64 / recall_duration.as_secs_f64();
   
   println!("Recall performance: {} recalls in {:?} ({:.2} recalls/sec)", 
            num_recalls, recall_duration, recalls_per_second);
   
   assert!(recalls_per_second > 30.0, "Recall performance too slow: {:.2} recalls/sec", recalls_per_second);
   
   // Test search performance
   let search_start = std::time::Instant::now();
   let num_searches = 50;
   
   for i in 0..num_searches {
       let query = format!("memory {}", i % 10);
       env.memory_manager.search_memories(user_id, &query, Some(20), Some(0))
           .expect("Should search memories");
   }
   
   let search_duration = search_start.elapsed();
   let searches_per_second = num_searches as f64 / search_duration.as_secs_f64();
   
   println!("Search performance: {} searches in {:?} ({:.2} searches/sec)", 
            num_searches, search_duration, searches_per_second);
   
   assert!(searches_per_second > 20.0, "Search performance too slow: {:.2} searches/sec", searches_per_second);
   
   // Verify all memories were saved correctly
   let all_memories = env.memory_manager.recall_memories(QueryFilter {
       user_id: Some(user_id.to_string()),
       limit: Some(2000), // Get all memories
       ..Default::default()
   }).expect("Should get all memories");
   
   assert_eq!(all_memories.total_count as usize, num_saves);
}

#[test]
#[serial]
fn test_error_recovery_and_resilience() {
   let env = TestEnvironment::new();
   
   // Test recovery from various error conditions
   
   // 1. Invalid memory operations
   let invalid_memory = MemoryItem {
       user_id: "".to_string(), // Invalid
       session_id: "test_session".to_string(),
       content: "".to_string(), // Invalid
       importance: -0.5, // Invalid
       ..Default::default()
   };
   
   assert!(env.memory_manager.save_memory(invalid_memory).is_err());
   
   // 2. Operations on non-existent entities
   assert!(env.memory_manager.get_memory("nonexistent_id").unwrap().is_none());
   assert!(!env.memory_manager.delete_memory("nonexistent_id").unwrap());
   
   let update = crate::core::memory::MemoryUpdate {
       content: Some("Updated".to_string()),
       importance: None,
       metadata: None,
       ttl_hours: None,
   };
   assert!(!env.memory_manager.update_memory("nonexistent_id", update).unwrap());
   
   // 3. Session operations with invalid data
   assert!(env.session_manager.create_session("", None).is_err());
   assert!(env.session_manager.generate_session_summary("nonexistent_session").is_err());
   
   // 4. Empty/boundary condition queries
   let empty_results = env.memory_manager.recall_memories(QueryFilter {
       user_id: Some("nonexistent_user".to_string()),
       ..Default::default()
   }).expect("Should handle nonexistent user");
   assert_eq!(empty_results.data.len(), 0);
   assert_eq!(empty_results.total_count, 0);
   
   // 5. Extreme pagination
   let beyond_limit = env.memory_manager.recall_memories(QueryFilter {
       user_id: Some("test_user".to_string()),
       offset: Some(999999),
       limit: Some(10),
       ..Default::default()
   }).expect("Should handle extreme pagination");
   assert_eq!(beyond_limit.data.len(), 0);
   
   // 6. Large content handling
   let large_content = "A".repeat(100_000); // 100KB content
   let large_memory = MemoryItem {
       user_id: "large_user".to_string(),
       session_id: "large_session".to_string(),
       content: large_content.clone(),
       importance: 0.5,
       ..Default::default()
   };
   
   let large_id = env.memory_manager.save_memory(large_memory).expect("Should handle large content");
   let retrieved = env.memory_manager.get_memory(&large_id).expect("Should retrieve large content");
   assert!(retrieved.is_some());
   assert_eq!(retrieved.unwrap().content.len(), 100_000);
   
   // 7. Unicode and special character handling
   let unicode_content = "测试 🚀 café naïve résumé Москва العالم";
   let unicode_memory = MemoryItem {
       user_id: "unicode_user".to_string(),
       session_id: "unicode_session".to_string(),
       content: unicode_content.to_string(),
       importance: 0.5,
       ..Default::default()
   };
   
   let unicode_id = env.memory_manager.save_memory(unicode_memory).expect("Should handle Unicode");
   let unicode_retrieved = env.memory_manager.get_memory(&unicode_id).expect("Should retrieve Unicode");
   assert!(unicode_retrieved.is_some());
   assert_eq!(unicode_retrieved.unwrap().content, unicode_content);
}

#[test]
#[serial]
fn test_end_to_end_user_workflow() {
   let env = TestEnvironment::new();
   
   // Simulate a realistic user workflow
   let user_id = "workflow_user";
   
   // Day 1: User starts trading
   let trading_session = env.session_manager
       .create_session(user_id, Some("My Trading Journal".to_string()))
       .expect("Should create trading session");
   
   // Morning: Research and planning
   let morning_memories = vec![
       "Market gapped up overnight on positive earnings news",
       "AAPL showing strong premarket volume, watching for breakout",
       "Fed meeting minutes released today at 2pm EST - key catalyst",
       "Portfolio currently 75% long, looking to add defensive positions",
   ];
   
   for content in morning_memories {
       let memory = MemoryItem {
           user_id: user_id.to_string(),
           session_id: trading_session.clone(),
           content: content.to_string(),
           importance: 0.7,
           metadata: {
               let mut meta = HashMap::new();
               meta.insert("time_of_day".to_string(), "morning".to_string());
               meta.insert("type".to_string(), "research".to_string());
               meta
           },
           ..Default::default()
       };
       env.memory_manager.save_memory(memory).expect("Should save morning memory");
   }
   
   // Afternoon: Trading activity
   let trading_memories = vec![
       ("Bought AAPL 100 shares at $175.50", 0.9),
       ("Set stop loss for AAPL at $172.00", 0.9),
       ("Fed minutes dovish, markets rallying", 0.8),
       ("Added QQQ calls, strike $350, exp Friday", 0.8),
       ("Portfolio now 85% long, feeling exposed", 0.6),
   ];
   
   for (content, importance) in trading_memories {
       let memory = MemoryItem {
           user_id: user_id.to_string(),
           session_id: trading_session.clone(),
           content: content.to_string(),
           importance,
           metadata: {
               let mut meta = HashMap::new();
               meta.insert("time_of_day".to_string(), "afternoon".to_string());
               meta.insert("type".to_string(), "trading".to_string());
               meta
           },
           ..Default::default()
       };
       env.memory_manager.save_memory(memory).expect("Should save trading memory");
   }
   
   // Evening: Review and planning
   let evening_memory = MemoryItem {
       user_id: user_id.to_string(),
       session_id: trading_session.clone(),
       content: "Good trading day. Up 2.3% on the day. Need to take some profits tomorrow.".to_string(),
       importance: 0.8,
       metadata: {
           let mut meta = HashMap::new();
           meta.insert("time_of_day".to_string(), "evening".to_string());
           meta.insert("type".to_string(), "review".to_string());
           meta
       },
       ..Default::default()
   };
   env.memory_manager.save_memory(evening_memory).expect("Should save evening memory");
   
   // Day 2: User wants to review yesterday's activities
   
   // Search for trading-related memories
   let trading_search = env.memory_manager
       .search_memories(user_id, "AAPL trading", Some(10), Some(0))
       .expect("Should find AAPL memories");
   assert!(trading_search.data.len() >= 2);
   assert!(trading_search.data.iter().any(|m| m.content.contains("175.50")));
   
   // Get high-importance memories (key decisions)
   let important_memories = env.memory_manager.recall_memories(QueryFilter {
       user_id: Some(user_id.to_string()),
       min_importance: Some(0.8),
       ..Default::default()
   }).expect("Should get important memories");
   assert!(important_memories.data.len() >= 4);
   
   // Generate session summary
   let session_summary = env.session_manager
       .generate_session_summary(&trading_session)
       .expect("Should generate session summary");
   
   assert!(!session_summary.summary_text.is_empty());
   assert!(session_summary.memory_count >= 10);
   assert!(session_summary.importance_score > 0.6);
   assert!(session_summary.key_topics.len() > 0);
   
   // User creates a new session for different strategy
   let strategy_session = env.session_manager
       .create_session(user_id, Some("Long-term Investment Strategy".to_string()))
       .expect("Should create strategy session");
   
   let strategy_memories = vec![
       "Research shows value stocks outperform in rising rate environment",
       "Consider increasing allocation to financials and energy sectors",
       "Reduce growth stock exposure from 60% to 40%",
       "Build position in dividend aristocrats for income",
   ];
   
   for content in strategy_memories {
       let memory = MemoryItem {
           user_id: user_id.to_string(),
           session_id: strategy_session.clone(),
           content: content.to_string(),
           importance: 0.7,
           metadata: {
               let mut meta = HashMap::new();
               meta.insert("strategy".to_string(), "long_term".to_string());
               meta
           },
           ..Default::default()
       };
       env.memory_manager.save_memory(memory).expect("Should save strategy memory");
   }
   
   // User reviews all sessions
   let all_sessions = env.session_manager
       .get_user_sessions(user_id, Some(10), Some(0))
       .expect("Should get all sessions");
   assert_eq!(all_sessions.data.len(), 2);
   
   let trading_session_data = all_sessions.data.iter()
       .find(|s| s.name.as_ref().unwrap().contains("Trading"))
       .expect("Should find trading session");
   assert!(trading_session_data.memory_count >= 10);
   
   let strategy_session_data = all_sessions.data.iter()
       .find(|s| s.name.as_ref().unwrap().contains("Strategy"))
       .expect("Should find strategy session");
   assert_eq!(strategy_session_data.memory_count, 4);
   
   // User exports data for backup
   let exported_data = env.memory_manager
       .export_user_memories(user_id)
       .expect("Should export user data");
   
   let parsed_memories: Vec<MemoryItem> = serde_json::from_str(&exported_data)
       .expect("Should parse exported data");
   assert!(parsed_memories.len() >= 14); // 10 + 4 memories
   
   // User gets analytics
   let user_stats = env.memory_manager
       .get_user_memory_stats(user_id)
       .expect("Should get user stats");
   assert!(user_stats.total_memories >= 14);
   assert!(user_stats.avg_importance > 0.6);
   
   let session_analytics = env.session_manager
       .get_session_analytics(user_id)
       .expect("Should get session analytics");
   assert_eq!(session_analytics.total_sessions, 2);
   assert!(session_analytics.total_memories >= 14);
   
   println!("End-to-end workflow completed successfully!");
   println!("User {} has {} memories across {} sessions", 
            user_id, user_stats.total_memories, session_analytics.total_sessions);
}