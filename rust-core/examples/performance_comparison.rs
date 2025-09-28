//! Performance comparison between sync and async operations

use anyhow::Result;
use mindcache_core::core::memory::MemoryManager;
use mindcache_core::core::{MindCacheConfig, RequestValidator};
use mindcache_core::database::models::*;
use mindcache_core::database::{Database, DatabaseConfig};
use std::time::Instant;

#[cfg(feature = "async")]
use mindcache_core::core::async_memory::AsyncMemoryManager;
#[cfg(feature = "async")]
use mindcache_core::database::async_db::AsyncDatabase;

fn main() -> Result<()> {
    env_logger::init();

    println!("⚡ MindCache Performance Comparison");
    println!("==================================\n");

    // Test configuration
    let num_operations = 100;
    let batch_size = 10;

    // Setup sync version
    println!("🔧 Setting up synchronous version...");
    let sync_result = setup_sync_test(num_operations)?;

    // Setup async version if available
    #[cfg(feature = "async")]
    {
        println!("🔧 Setting up asynchronous version...");
        let async_result =
            tokio::runtime::Runtime::new()?.block_on(setup_async_test(num_operations))?;

        compare_results(sync_result, async_result, num_operations);
    }

    #[cfg(not(feature = "async"))]
    {
        println!(
            "⚠️  Async features not enabled. Compile with --features async for full comparison."
        );
        println!("\nSync results:");
        print_test_results("Synchronous", sync_result, num_operations);
    }

    Ok(())
}

#[derive(Debug)]
struct TestResults {
    setup_time_ms: u128,
    save_time_ms: u128,
    recall_time_ms: u128,
    batch_time_ms: u128,
    total_time_ms: u128,
}

fn setup_sync_test(num_operations: usize) -> Result<TestResults> {
    let start = Instant::now();

    let db_config = DatabaseConfig {
        path: ":memory:".to_string(),
        max_connections: 4,
        ..Default::default()
    };

    let database = Database::new(db_config)?;
    let config = MindCacheConfig::default();
    let validator = RequestValidator::new(&config);
    let manager = MemoryManager::new(database, validator);

    let setup_time = start.elapsed();

    // Test individual saves
    let save_start = Instant::now();
    let mut memory_ids = Vec::new();

    for i in 0..num_operations {
        let memory = MemoryItem {
            user_id: "perf_user".to_string(),
            session_id: "perf_session".to_string(),
            content: format!("Performance test memory {}", i),
            importance: 0.5,
            ..Default::default()
        };

        let id = manager.save_memory(memory)?;
        memory_ids.push(id);
    }

    let save_time = save_start.elapsed();

    // Test recalls
    let recall_start = Instant::now();

    for _ in 0..num_operations / 10 {
        let filter = QueryFilter {
            user_id: Some("perf_user".to_string()),
            limit: Some(10),
            ..Default::default()
        };

        let _results = manager.recall_memories(filter)?;
    }

    let recall_time = recall_start.elapsed();

    // Test batch operations
    let batch_start = Instant::now();

    let batch_memories: Vec<MemoryItem> = (0..num_operations / 10)
        .map(|i| MemoryItem {
            user_id: "batch_user".to_string(),
            session_id: "batch_session".to_string(),
            content: format!("Batch memory {}", i),
            importance: 0.5,
            ..Default::default()
        })
        .collect();

    let batch_request = crate::core::BatchRequest {
        items: batch_memories,
        fail_on_error: false,
    };

    let _batch_response = manager.save_memories_batch(batch_request)?;

    let batch_time = batch_start.elapsed();
    let total_time = start.elapsed();

    Ok(TestResults {
        setup_time_ms: setup_time.as_millis(),
        save_time_ms: save_time.as_millis(),
        recall_time_ms: recall_time.as_millis(),
        batch_time_ms: batch_time.as_millis(),
        total_time_ms: total_time.as_millis(),
    })
}

#[cfg(feature = "async")]
async fn setup_async_test(num_operations: usize) -> Result<TestResults> {
    let start = Instant::now();

    let db_config = DatabaseConfig {
        path: ":memory:".to_string(),
        max_connections: 4,
        ..Default::default()
    };

    let async_db = AsyncDatabase::new(db_config).await?;
    let config = MindCacheConfig::default();
    let validator = RequestValidator::new(&config);
    let manager = AsyncMemoryManager::new(async_db, validator);

    let setup_time = start.elapsed();

    // Test individual saves
    let save_start = Instant::now();
    let mut save_tasks = Vec::new();

    for i in 0..num_operations {
        let memory = MemoryItem {
            user_id: "perf_user".to_string(),
            session_id: "perf_session".to_string(),
            content: format!("Performance test memory {}", i),
            importance: 0.5,
            ..Default::default()
        };

        save_tasks.push(manager.save_memory(memory));
    }

    let _memory_ids: Vec<_> = futures::future::join_all(save_tasks)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    let save_time = save_start.elapsed();

    // Test recalls
    let recall_start = Instant::now();
    let mut recall_tasks = Vec::new();

    for _ in 0..num_operations / 10 {
        let filter = QueryFilter {
            user_id: Some("perf_user".to_string()),
            limit: Some(10),
            ..Default::default()
        };

        recall_tasks.push(manager.recall_memories(filter));
    }

    let _recall_results: Vec<_> = futures::future::join_all(recall_tasks)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    let recall_time = recall_start.elapsed();

    // Test batch operations
    let batch_start = Instant::now();

    let batch_memories: Vec<MemoryItem> = (0..num_operations / 10)
        .map(|i| MemoryItem {
            user_id: "batch_user".to_string(),
            session_id: "batch_session".to_string(),
            content: format!("Batch memory {}", i),
            importance: 0.5,
            ..Default::default()
        })
        .collect();

    let batch_request = crate::core::BatchRequest {
        items: batch_memories,
        fail_on_error: false,
    };

    let _batch_response = manager.save_memories_batch(batch_request).await?;

    let batch_time = batch_start.elapsed();
    let total_time = start.elapsed();

    Ok(TestResults {
        setup_time_ms: setup_time.as_millis(),
        save_time_ms: save_time.as_millis(),
        recall_time_ms: recall_time.as_millis(),
        batch_time_ms: batch_time.as_millis(),
        total_time_ms: total_time.as_millis(),
    })
}

#[cfg(feature = "async")]
fn compare_results(sync_results: TestResults, async_results: TestResults, num_operations: usize) {
    println!("📊 Performance Comparison Results");
    println!("=================================\n");

    print_comparison_table(&sync_results, &async_results, num_operations);

    println!("\n📈 Performance Analysis:");
    println!("========================");

    let sync_ops_per_sec = (num_operations as f64 * 1000.0) / sync_results.save_time_ms as f64;
    let async_ops_per_sec = (num_operations as f64 * 1000.0) / async_results.save_time_ms as f64;

    println!("• Save throughput:");
    println!("  - Sync:  {:.1} ops/sec", sync_ops_per_sec);
    println!("  - Async: {:.1} ops/sec", async_ops_per_sec);

    if async_ops_per_sec > sync_ops_per_sec {
        let improvement = (async_ops_per_sec / sync_ops_per_sec - 1.0) * 100.0;
        println!("  - Async is {:.1}% faster", improvement);
    } else {
        let degradation = (sync_ops_per_sec / async_ops_per_sec - 1.0) * 100.0;
        println!("  - Async is {:.1}% slower", degradation);
    }

    println!("\n• Total time improvement:");
    if async_results.total_time_ms < sync_results.total_time_ms {
        let improvement = ((sync_results.total_time_ms - async_results.total_time_ms) as f64
            / sync_results.total_time_ms as f64)
            * 100.0;
        println!("  - Async is {:.1}% faster overall", improvement);
    } else {
        let degradation = ((async_results.total_time_ms - sync_results.total_time_ms) as f64
            / sync_results.total_time_ms as f64)
            * 100.0;
        println!("  - Async is {:.1}% slower overall", degradation);
    }

    println!("\n💡 Recommendations:");
    println!("===================");

    if async_ops_per_sec > sync_ops_per_sec * 1.2 {
        println!("• Use async operations for high-throughput scenarios");
        println!("• Async excels at concurrent operations");
    } else if sync_ops_per_sec > async_ops_per_sec * 1.1 {
        println!("• Sync operations may be better for simple, sequential tasks");
        println!("• Consider async overhead for small workloads");
    } else {
        println!("• Performance is similar - choose based on application architecture");
        println!("• Async provides better scalability for concurrent users");
    }
}

#[cfg(feature = "async")]
fn print_comparison_table(
    sync_results: &TestResults,
    async_results: &TestResults,
    num_operations: usize,
) {
    println!("┌──────────────────┬──────────────┬──────────────┬────────────────┐");
    println!("│ Operation        │ Sync (ms)    │ Async (ms)   │ Difference     │");
    println!("├──────────────────┼──────────────┼──────────────┼────────────────┤");

    print_table_row(
        "Setup",
        sync_results.setup_time_ms,
        async_results.setup_time_ms,
    );
    print_table_row(
        "Individual Saves",
        sync_results.save_time_ms,
        async_results.save_time_ms,
    );
    print_table_row(
        "Recalls",
        sync_results.recall_time_ms,
        async_results.recall_time_ms,
    );
    print_table_row(
        "Batch Operations",
        sync_results.batch_time_ms,
        async_results.batch_time_ms,
    );
    print_table_row(
        "Total",
        sync_results.total_time_ms,
        async_results.total_time_ms,
    );

    println!("└──────────────────┴──────────────┴──────────────┴────────────────┘");

    println!(
        "\nOperations: {} saves, {} recalls, {} batch items",
        num_operations,
        num_operations / 10,
        num_operations / 10
    );
}

#[cfg(feature = "async")]
fn print_table_row(operation: &str, sync_time: u128, async_time: u128) {
    let diff = if async_time < sync_time {
        format!(
            "-{:.1}%",
            ((sync_time - async_time) as f64 / sync_time as f64) * 100.0
        )
    } else {
        format!(
            "+{:.1}%",
            ((async_time - sync_time) as f64 / sync_time as f64) * 100.0
        )
    };

    println!(
        "│ {:<16} │ {:>12} │ {:>12} │ {:>14} │",
        operation, sync_time, async_time, diff
    );
}

fn print_test_results(name: &str, results: TestResults, num_operations: usize) {
    println!("🔍 {} Results:", name);
    println!("  Setup time: {}ms", results.setup_time_ms);
    println!(
        "  Save {} items: {}ms ({:.1} ops/sec)",
        num_operations,
        results.save_time_ms,
        (num_operations as f64 * 1000.0) / results.save_time_ms as f64
    );
    println!("  Recall operations: {}ms", results.recall_time_ms);
    println!("  Batch operations: {}ms", results.batch_time_ms);
    println!("  Total time: {}ms", results.total_time_ms);
}
