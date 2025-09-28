//! Demonstration of async operations with vector search

use anyhow::Result;
#[cfg(all(feature = "async", feature = "vector-search"))]
use mindcache_core::core::async_memory::AsyncMemoryManager;
#[cfg(all(feature = "async", feature = "vector-search"))]
use mindcache_core::core::{BatchRequest, MindCacheConfig, RequestValidator};
#[cfg(all(feature = "async", feature = "vector-search"))]
use mindcache_core::database::models::*;
#[cfg(all(feature = "async", feature = "vector-search"))]
use mindcache_core::database::vector::VectorConfig;
#[cfg(all(feature = "async", feature = "vector-search"))]
use mindcache_core::database::{async_db::AsyncDatabase, DatabaseConfig};
use std::collections::HashMap;
use std::io::Write; // Add this import for flush()

#[cfg(all(feature = "async", feature = "vector-search"))]
#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("🚀 MindCache Async + Vector Search Demo");
    println!("=======================================\n");

    // Setup async database with vector support
    let db_config = DatabaseConfig {
        path: "./examples/async_vector_demo.db".to_string(),
        max_connections: 8,
        min_connections: 2,
        ..Default::default()
    };

    let vector_config = VectorConfig {
        dimension: 384, // Common embedding dimension
        similarity_threshold: 0.7,
        max_results: 20,
        enable_approximate_search: true,
    };

    println!("📊 Initializing async database with vector search...");
    let async_db = AsyncDatabase::new_with_vector(db_config, vector_config).await?;

    let config = MindCacheConfig::default();
    let validator = RequestValidator::new(&config);
    let memory_manager = AsyncMemoryManager::new(async_db, validator);

    println!("✅ Database initialized successfully\n");

    // Demo 1: Concurrent memory operations
    println!("🔄 Demo 1: Concurrent Memory Operations");
    println!("--------------------------------------");

    let user_id = "demo_user";
    let session_id = "demo_session";

    // Create multiple memories concurrently
    let memory_tasks: Vec<_> = (0..5)
        .map(|i| {
            let manager = memory_manager.clone();
            let user_id = user_id.to_string();
            let session_id = session_id.to_string();

            tokio::spawn(async move {
                let mut metadata = HashMap::new();
                metadata.insert("demo_id".to_string(), i.to_string());
                metadata.insert(
                    "category".to_string(),
                    if i % 2 == 0 { "tech" } else { "science" }.to_string(),
                );

                let memory = MemoryItem {
                    user_id,
                    session_id,
                    content: format!(
                        "This is async demo memory #{} about {}",
                        i,
                        if i % 2 == 0 {
                            "artificial intelligence and machine learning"
                        } else {
                            "quantum physics and space exploration"
                        }
                    ),
                    importance: 0.5 + (i as f32 * 0.1),
                    metadata,
                    // Simulate embeddings (in real use, these would come from an embedding model)
                    embedding: Some(generate_mock_embedding(i)),
                    embedding_model: Some("demo_model_v1".to_string()),
                    ..Default::default()
                };

                manager.save_memory(memory).await
            })
        })
        .collect();

    // Wait for all saves to complete
    let results = futures::future::join_all(memory_tasks).await;
    let memory_ids: Vec<String> = results
        .into_iter()
        .filter_map(|r| r.ok().and_then(|r| r.ok()))
        .collect();

    println!("✅ Saved {} memories concurrently", memory_ids.len());

    // Store embeddings for vector search
    println!("\n🧠 Storing vector embeddings...");
    for (i, memory_id) in memory_ids.iter().enumerate() {
        let embedding = generate_mock_embedding(i);
        memory_manager
            .store_embedding(memory_id, embedding, "demo_model_v1")
            .await?;
    }
    println!("✅ Stored {} embeddings", memory_ids.len());

    // Demo 2: Vector similarity search
    println!("\n🔍 Demo 2: Vector Similarity Search");
    println!("-----------------------------------");

    let query_embedding = generate_mock_embedding(0); // Similar to tech memories
    let similar_memories = memory_manager
        .search_similar(user_id, query_embedding, "demo_model_v1", Some(3))
        .await?;

    println!("Found {} similar memories:", similar_memories.len());
    for (i, result) in similar_memories.iter().enumerate() {
        println!(
            "  {}. Similarity: {:.3} | {}",
            i + 1,
            result.similarity,
            if result.content.len() > 60 {
                format!("{}...", &result.content[..60])
            } else {
                result.content.clone()
            }
        );
    }

    // Demo 3: Hybrid search (text + vector)
    println!("\n🔀 Demo 3: Hybrid Search");
    println!("-----------------------");

    let text_query = "artificial intelligence";
    let vector_query = generate_mock_embedding(0);

    let hybrid_results = memory_manager
        .hybrid_search(
            user_id,
            text_query,
            vector_query,
            "demo_model_v1",
            0.6, // text weight
            0.4, // vector weight
            Some(3),
        )
        .await?;

    println!("Hybrid search results for '{}':", text_query);
    for (i, result) in hybrid_results.iter().enumerate() {
        println!(
            "  {}. Score: {:.3} (text: {:.1}, vector: {:.3})",
            i + 1,
            result.combined_score,
            result.text_match,
            result.vector_similarity
        );
        println!(
            "     Content: {}",
            if result.content.len() > 70 {
                format!("{}...", &result.content[..70])
            } else {
                result.content.clone()
            }
        );
    }

    // Demo 4: Async batch operations with progress
    println!("\n📦 Demo 4: Batch Operations with Progress");
    println!("----------------------------------------");

    let batch_memories: Vec<MemoryItem> = (0..20)
        .map(|i| {
            MemoryItem {
                user_id: user_id.to_string(),
                session_id: format!("batch_session_{}", i / 5), // 4 sessions with 5 memories each
                content: format!(
                    "Batch memory {} discussing various topics in technology and science",
                    i
                ),
                importance: fastrand::f32() * 0.6 + 0.2, // Random importance between 0.2 and 0.8
                ..Default::default()
            }
        })
        .collect();

    println!("Saving {} memories in batch...", batch_memories.len());

    let batch_request = BatchRequest {
        items: batch_memories,
        fail_on_error: false,
    };

    let batch_response = memory_manager.save_memories_batch(batch_request).await?;

    println!(
        "✅ Batch completed: {}/{} successful, {} errors",
        batch_response.success_count,
        batch_response.results.len(),
        batch_response.error_count
    );
    println!(
        "   Success rate: {:.1}%",
        batch_response.success_rate() * 100.0
    );

    // Demo 5: Export with progress tracking
    println!("\n📤 Demo 5: Export with Progress Tracking");
    println!("---------------------------------------");

    let exported_memories = memory_manager
        .export_user_memories_with_progress(user_id, |current, total| {
            let progress = (current as f32 / total as f32) * 100.0;
            print!(
                "\rExporting memories: {:.1}% ({}/{})",
                progress, current, total
            );
            std::io::stdout().flush().ok();
        })
        .await?;

    println!(
        "\n✅ Exported {} memories for user {}",
        exported_memories.len(),
        user_id
    );

    // Demo 6: Performance metrics
    println!("\n📈 Demo 6: Performance Metrics");
    println!("-----------------------------");

    let metrics = memory_manager.get_performance_metrics().await;
    println!("Average save time: {:.2}ms", metrics.avg_save_time_ms);
    println!("Average query time: {:.2}ms", metrics.avg_query_time_ms);
    println!(
        "Operations per second: {:.1} saves, {:.1} queries",
        metrics.saves_per_second, metrics.queries_per_second
    );

    // Demo 7: Memory statistics
    println!("\n📊 Demo 7: User Memory Statistics");
    println!("--------------------------------");

    let stats = memory_manager.get_user_memory_stats(user_id).await?;
    println!("Total memories: {}", stats.total_memories);
    println!("Average importance: {:.2}", stats.avg_importance);

    if let (Some(oldest), Some(newest)) = (stats.oldest_memory, stats.newest_memory) {
        let age_span = newest - oldest;
        println!("Memory span: {} seconds", age_span.num_seconds());
    }

    println!("\nImportance distribution:");
    for (category, count) in &stats.importance_distribution {
        println!("  {}: {}", category, count);
    }

    println!("\n🎉 Demo completed successfully!");
    println!("Database saved to: ./examples/async_vector_demo.db");
    println!("You can inspect the database using the CLI:");
    println!("  cargo run --features async,vector-search -- --database ./examples/async_vector_demo.db --enable-vector memory recall --user demo_user");

    Ok(())
}

#[cfg(not(all(feature = "async", feature = "vector-search")))]
fn main() {
    println!("This example requires both 'async' and 'vector-search' features.");
    println!("Run with: cargo run --example async_vector_demo --features async,vector-search");
}

/// Generate a mock embedding vector for demonstration
#[cfg(all(feature = "async", feature = "vector-search"))]
fn generate_mock_embedding(seed: usize) -> Vec<f32> {
    let mut embedding = Vec::with_capacity(384);

    // Create deterministic but varied embeddings based on seed
    for i in 0..384 {
        let value = ((seed as f32 + i as f32) * 0.01).sin() * 0.5;
        embedding.push(value);
    }

    // Normalize the vector
    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for val in &mut embedding {
            *val /= magnitude;
        }
    }

    embedding
}
