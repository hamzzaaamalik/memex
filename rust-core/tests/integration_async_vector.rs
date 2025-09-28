//! Integration tests for async and vector search functionality

#[cfg(all(feature = "async", feature = "vector-search"))]
mod async_vector_tests {
    use super::super::*;
    use mindcache_core::database::async_db::AsyncDatabase;
    use mindcache_core::database::vector::VectorConfig;
    use mindcache_core::core::async_memory::AsyncMemoryManager;
    use mindcache_core::core::{MindCacheConfig, RequestValidator};
    use mindcache_core::database::models::*;
    use tempfile::TempDir;
    use tokio;

    async fn setup_async_test_environment() -> (AsyncMemoryManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_config = DatabaseConfig {
            path: temp_dir.path().join("test.db").to_string_lossy().to_string(),
            max_connections: 4,
            min_connections: 1,
            ..Default::default()
        };

        let vector_config = VectorConfig {
            dimension: 4, // Small dimension for testing
            similarity_threshold: 0.5,
            max_results: 10,
            enable_approximate_search: false,
        };

        let async_db = AsyncDatabase::new_with_vector(db_config, vector_config).await.unwrap();
        let config = MindCacheConfig::default();
        let validator = RequestValidator::new(&config);
        let manager = AsyncMemoryManager::new(async_db, validator);

        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_async_memory_operations() {
        let (manager, _temp_dir) = setup_async_test_environment().await;

        // Test async save
        let memory = MemoryItem {
            user_id: "async_user".to_string(),
            session_id: "async_session".to_string(),
            content: "Async test memory".to_string(),
            importance: 0.8,
            embedding: Some(vec![0.1, 0.2, 0.3, 0.4]),
            embedding_model: Some("test_model".to_string()),
            ..Default::default()
        };

        let memory_id = manager.save_memory(memory.clone()).await.unwrap();
        assert!(!memory_id.is_empty());

        // Test async retrieval
        let retrieved = manager.get_memory(&memory_id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.content, memory.content);
        assert_eq!(retrieved.embedding, memory.embedding);

        // Test async search
        let filter = QueryFilter {
            user_id: Some("async_user".to_string()),
            keywords: Some(vec!["Async".to_string()]),
            ..Default::default()
        };

        let results = manager.recall_memories(filter).await.unwrap();
        assert_eq!(results.data.len(), 1);
        assert_eq!(results.data[0].id, memory_id);
    }

    #[tokio::test]
    async fn test_async_batch_operations() {
        let (manager, _temp_dir) = setup_async_test_environment().await;

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
        ];

        let batch_request = crate::core::BatchRequest {
            items: memories,
            fail_on_error: false,
        };

        let response = manager.save_memories_batch(batch_request).await.unwrap();
        assert_eq!(response.success_count, 2);
        assert_eq!(response.error_count, 0);
    }

    #[tokio::test]
    async fn test_vector_search_integration() {
        let (manager, _temp_dir) = setup_async_test_environment().await;

        // Store memories with embeddings
        let memories_data = vec![
            ("Memory about cats", vec![1.0, 0.0, 0.0, 0.0]),
            ("Memory about dogs", vec![0.0, 1.0, 0.0, 0.0]),
            ("Memory about pets", vec![0.5, 0.5, 0.0, 0.0]),
        ];

        let mut memory_ids = Vec::new();
        for (content, embedding) in memories_data {
            let memory = MemoryItem {
                user_id: "vector_user".to_string(),
                session_id: "vector_session".to_string(),
                content: content.to_string(),
                importance: 0.7,
                ..Default::default()
            };

            let memory_id = manager.save_memory(memory).await.unwrap();
            manager.store_embedding(&memory_id, embedding, "test_model").await.unwrap();
            memory_ids.push(memory_id);
        }

        // Test vector similarity search
        let query_embedding = vec![1.0, 0.1, 0.0, 0.0]; // Similar to cats
        let results = manager.search_similar("vector_user", query_embedding, "test_model", Some(5)).await.unwrap();

        assert!(!results.is_empty());
        // First result should be most similar (cats)
        assert!(results[0].content.contains("cats"));
        assert!(results[0].similarity > 0.9);
    }

    #[tokio::test]
    async fn test_hybrid_search() {
        let (manager, _temp_dir) = setup_async_test_environment().await;

        // Store memory with both text and vector
        let memory = MemoryItem {
            user_id: "hybrid_user".to_string(),
            session_id: "hybrid_session".to_string(),
            content: "Apple fruit is healthy and delicious".to_string(),
            importance: 0.8,
            ..Default::default()
        };

        let memory_id = manager.save_memory(memory).await.unwrap();
        let embedding = vec![0.8, 0.2, 0.1, 0.1]; // Fruit-like embedding
        manager.store_embedding(&memory_id, embedding, "test_model").await.unwrap();

        // Test hybrid search
        let text_query = "apple";
        let vector_query = vec![0.7, 0.3, 0.1, 0.1]; // Similar to stored embedding
        
        let results = manager.hybrid_search(
            "hybrid_user",
            text_query,
            vector_query,
            "test_model",
            0.5, // text weight
            0.5, // vector weight
            Some(5)
        ).await.unwrap();

        assert!(!results.is_empty());
        assert!(results[0].content.contains("Apple"));
        assert!(results[0].combined_score > 0.5);
        assert!(results[0].text_match > 0.0);
        assert!(results[0].vector_similarity > 0.8);
    }

    #[tokio::test]
    async fn test_concurrent_async_operations() {
        let (manager, _temp_dir) = setup_async_test_environment().await;

        // Test concurrent saves
        let tasks: Vec<_> = (0..10).map(|i| {
            let manager = manager.clone();
            tokio::spawn(async move {
                let memory = MemoryItem {
                    user_id: format!("concurrent_user_{}", i % 3),
                    session_id: "concurrent_session".to_string(),
                    content: format!("Concurrent memory {}", i),
                    importance: 0.5,
                    ..Default::default()
                };
                manager.save_memory(memory).await
            })
        }).collect();

        // Wait for all tasks to complete
        let results = futures::future::join_all(tasks).await;
        
        // Most should succeed
        let success_count = results.iter().filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok()).count();
        assert!(success_count >= 8, "Expected at least 8 successful operations, got {}", success_count);
    }

    #[tokio::test]
    async fn test_performance_monitoring() {
        let (manager, _temp_dir) = setup_async_test_environment().await;

        // Perform operations to generate metrics
        for i in 0..5 {
            let memory = MemoryItem {
                user_id: "perf_user".to_string(),
                session_id: "perf_session".to_string(),
                content: format!("Performance test memory {}", i),
                ..Default::default()
            };
            manager.save_memory(memory).await.unwrap();
        }

        // Get performance metrics
        let metrics = manager.get_performance_metrics().await;
        assert!(metrics.avg_save_time_ms > 0.0);
    }
}