//! Performance benchmarks for MindCache

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use mindcache_core::core::memory::MemoryManager;
use mindcache_core::core::session::SessionManager;
use mindcache_core::core::{MindCacheConfig, RequestValidator};
use mindcache_core::database::models::*;
use mindcache_core::database::{Database, DatabaseConfig};
use std::collections::HashMap;
use tempfile::TempDir;

fn setup_benchmark_env() -> (MemoryManager, SessionManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_config = DatabaseConfig {
        path: temp_dir
            .path()
            .join("benchmark.db")
            .to_string_lossy()
            .to_string(),
        cache_size: -64000, // 64MB cache for benchmarks
        ..Default::default()
    };

    let database = Database::new(db_config).unwrap();
    let config = MindCacheConfig {
        enable_request_limits: false, // Disable for accurate benchmarking
        ..Default::default()
    };
    let validator = RequestValidator::new(&config);

    let memory_manager = MemoryManager::new(database.clone(), validator.clone());
    let session_manager = SessionManager::new(database, validator);

    (memory_manager, session_manager, temp_dir)
}

fn create_test_memory(
    user_id: &str,
    session_id: &str,
    content: &str,
    importance: f32,
) -> MemoryItem {
    MemoryItem {
        user_id: user_id.to_string(),
        session_id: session_id.to_string(),
        content: content.to_string(),
        importance,
        ..Default::default()
    }
}

fn bench_memory_save(c: &mut Criterion) {
    let (memory_manager, _, _temp_dir) = setup_benchmark_env();

    let mut group = c.benchmark_group("memory_save");

    // Benchmark different content sizes
    for content_size in [100, 1000, 10000].iter() {
        let content = "A".repeat(*content_size);
        let memory = create_test_memory("bench_user", "bench_session", &content, 0.5);

        group.bench_with_input(
            BenchmarkId::new("content_size", content_size),
            content_size,
            |b, _| {
                b.iter(|| {
                    let mem = memory.clone();
                    black_box(memory_manager.save_memory(mem).unwrap())
                })
            },
        );
    }

    // Benchmark different importance levels
    for importance in [0.1, 0.5, 0.9].iter() {
        let memory = create_test_memory(
            "bench_user",
            "bench_session",
            "Benchmark content",
            *importance,
        );

        group.bench_with_input(
            BenchmarkId::new("importance", format!("{:.1}", importance)),
            importance,
            |b, _| {
                b.iter(|| {
                    let mem = memory.clone();
                    black_box(memory_manager.save_memory(mem).unwrap())
                })
            },
        );
    }

    group.finish();
}

fn bench_memory_recall(c: &mut Criterion) {
    let (memory_manager, _, _temp_dir) = setup_benchmark_env();

    // Pre-populate with test data
    for i in 0..1000 {
        let memory = create_test_memory(
            "recall_user",
            "recall_session",
            &format!(
                "Recall benchmark memory {} with various keywords trading stocks crypto",
                i
            ),
            0.3 + (i % 7) as f32 * 0.1,
        );
        memory_manager.save_memory(memory).unwrap();
    }

    let mut group = c.benchmark_group("memory_recall");

    // Benchmark different filter types
    let filters = vec![
        (
            "no_filter",
            QueryFilter {
                user_id: Some("recall_user".to_string()),
                limit: Some(50),
                ..Default::default()
            },
        ),
        (
            "importance_filter",
            QueryFilter {
                user_id: Some("recall_user".to_string()),
                min_importance: Some(0.7),
                limit: Some(50),
                ..Default::default()
            },
        ),
        (
            "keyword_filter",
            QueryFilter {
                user_id: Some("recall_user".to_string()),
                keywords: Some(vec!["trading".to_string()]),
                limit: Some(50),
                ..Default::default()
            },
        ),
        (
            "combined_filter",
            QueryFilter {
                user_id: Some("recall_user".to_string()),
                keywords: Some(vec!["stocks".to_string()]),
                min_importance: Some(0.5),
                limit: Some(50),
                ..Default::default()
            },
        ),
    ];

    for (filter_name, filter) in filters {
        group.bench_function(filter_name, |b| {
            b.iter(|| black_box(memory_manager.recall_memories(filter.clone()).unwrap()))
        });
    }

    // Benchmark different page sizes
    for limit in [10, 50, 100, 500].iter() {
        let filter = QueryFilter {
            user_id: Some("recall_user".to_string()),
            limit: Some(*limit),
            ..Default::default()
        };

        group.bench_with_input(BenchmarkId::new("page_size", limit), limit, |b, _| {
            b.iter(|| black_box(memory_manager.recall_memories(filter.clone()).unwrap()))
        });
    }

    group.finish();
}

fn bench_full_text_search(c: &mut Criterion) {
    let (memory_manager, _, _temp_dir) = setup_benchmark_env();

    // Pre-populate with diverse content
    let content_templates = vec![
        "Apple Inc. stock analysis shows strong quarterly growth",
        "Bitcoin cryptocurrency market volatility creates opportunities",
        "Tesla electric vehicle deliveries exceed expectations",
        "Microsoft cloud computing revenue drives earnings beat",
        "Amazon e-commerce platform expansion into new markets",
        "Google advertising business faces regulatory scrutiny",
        "Meta virtual reality investments show promise",
        "NVIDIA artificial intelligence chips in high demand",
    ];

    for i in 0..500 {
        let content = format!(
            "{} - iteration {}",
            content_templates[i % content_templates.len()],
            i
        );
        let memory = create_test_memory("search_user", "search_session", &content, 0.5);
        memory_manager.save_memory(memory).unwrap();
    }

    let mut group = c.benchmark_group("full_text_search");

    let search_queries = vec![
        "Apple stock",
        "Bitcoin cryptocurrency",
        "Tesla vehicle",
        "Microsoft cloud",
        "artificial intelligence",
        "market volatility",
    ];

    for query in search_queries {
        group.bench_function(query, |b| {
            b.iter(|| {
                black_box(
                    memory_manager
                        .search_memories("search_user", query, Some(20), Some(0))
                        .unwrap(),
                )
            })
        });
    }

    group.finish();
}

fn bench_session_operations(c: &mut Criterion) {
    let (memory_manager, session_manager, _temp_dir) = setup_benchmark_env();

    // Create sessions with memories
    for i in 0..10 {
        let session_id = session_manager
            .create_session("session_user", Some(format!("Benchmark Session {}", i)))
            .unwrap();

        for j in 0..50 {
            let memory = create_test_memory(
                "session_user",
                &session_id,
                &format!("Session {} memory {} about trading and investments", i, j),
                0.4 + (j % 6) as f32 * 0.1,
            );
            memory_manager.save_memory(memory).unwrap();
        }
    }

    let mut group = c.benchmark_group("session_operations");

    group.bench_function("get_user_sessions", |b| {
        b.iter(|| {
            black_box(
                session_manager
                    .get_user_sessions("session_user", Some(20), Some(0))
                    .unwrap(),
            )
        })
    });

    // Get a session ID for summary benchmarking
    let sessions = session_manager
        .get_user_sessions("session_user", Some(1), Some(0))
        .unwrap();
    let session_id = &sessions.data[0].id;

    group.bench_function("generate_session_summary", |b| {
        b.iter(|| {
            black_box(
                session_manager
                    .generate_session_summary(session_id)
                    .unwrap(),
            )
        })
    });

    group.bench_function("search_sessions", |b| {
        b.iter(|| {
            black_box(
                session_manager
                    .search_sessions("session_user", vec!["trading".to_string()])
                    .unwrap(),
            )
        })
    });

    group.finish();
}

fn bench_batch_operations(c: &mut Criterion) {
    let (memory_manager, _, _temp_dir) = setup_benchmark_env();

    let mut group = c.benchmark_group("batch_operations");

    // Benchmark different batch sizes
    for batch_size in [10, 50, 100].iter() {
        let memories: Vec<MemoryItem> = (0..*batch_size)
            .map(|i| {
                create_test_memory(
                    "batch_user",
                    "batch_session",
                    &format!("Batch memory {}", i),
                    0.5,
                )
            })
            .collect();

        let batch_request = mindcache_core::core::BatchRequest {
            items: memories,
            fail_on_error: false,
        };

        group.bench_with_input(
            BenchmarkId::new("batch_size", batch_size),
            batch_size,
            |b, _| {
                b.iter(|| {
                    let req = batch_request.clone();
                    black_box(memory_manager.save_memories_batch(req).unwrap())
                })
            },
        );
    }

    group.finish();
}

fn bench_concurrent_operations(c: &mut Criterion) {
    let (memory_manager, _, _temp_dir) = setup_benchmark_env();

    let mut group = c.benchmark_group("concurrent_operations");

    // Simulate concurrent saves (sequential for benchmarking)
    group.bench_function("concurrent_saves", |b| {
        b.iter(|| {
            for i in 0..10 {
                let memory = create_test_memory(
                    &format!("concurrent_user_{}", i % 3),
                    "concurrent_session",
                    &format!("Concurrent memory {}", i),
                    0.5,
                );
                black_box(memory_manager.save_memory(memory).unwrap());
            }
        })
    });

    // Simulate concurrent recalls
    group.bench_function("concurrent_recalls", |b| {
        b.iter(|| {
            for i in 0..5 {
                let filter = QueryFilter {
                    user_id: Some(format!("concurrent_user_{}", i % 3)),
                    limit: Some(10),
                    ..Default::default()
                };
                black_box(memory_manager.recall_memories(filter).unwrap());
            }
        })
    });

    group.finish();
}

fn bench_data_scaling(c: &mut Criterion) {
    // Test performance with different data sizes
    let mut group = c.benchmark_group("data_scaling");

    for data_size in [100, 1000, 5000].iter() {
        let (memory_manager, _, _temp_dir) = setup_benchmark_env();

        // Pre-populate with test data
        for i in 0..*data_size {
            let memory = create_test_memory(
                "scaling_user",
                &format!("session_{}", i % 10),
                &format!(
                    "Scaling test memory {} with content about various topics",
                    i
                ),
                0.3 + (i % 7) as f32 * 0.1,
            );
            memory_manager.save_memory(memory).unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new("recall_with_data_size", data_size),
            data_size,
            |b, _| {
                let filter = QueryFilter {
                    user_id: Some("scaling_user".to_string()),
                    limit: Some(50),
                    ..Default::default()
                };
                b.iter(|| black_box(memory_manager.recall_memories(filter.clone()).unwrap()))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("search_with_data_size", data_size),
            data_size,
            |b, _| {
                b.iter(|| {
                    black_box(
                        memory_manager
                            .search_memories("scaling_user", "memory", Some(20), Some(0))
                            .unwrap(),
                    )
                })
            },
        );
    }

    group.finish();
}

fn bench_memory_update_operations(c: &mut Criterion) {
    let (memory_manager, _, _temp_dir) = setup_benchmark_env();

    // Pre-populate with memories to update
    let mut memory_ids = Vec::new();
    for i in 0..100 {
        let memory = create_test_memory(
            "update_user",
            "update_session",
            &format!("Memory to update {}", i),
            0.5,
        );
        let id = memory_manager.save_memory(memory).unwrap();
        memory_ids.push(id);
    }

    let mut group = c.benchmark_group("memory_updates");

    group.bench_function("update_content", |b| {
        let mut counter = 0;
        b.iter(|| {
            let update = mindcache_core::core::memory::MemoryUpdate {
                content: Some(format!("Updated content {}", counter)),
                importance: None,
                metadata: None,
                ttl_hours: None,
            };
            let id = &memory_ids[counter % memory_ids.len()];
            black_box(memory_manager.update_memory(id, update).unwrap());
            counter += 1;
        })
    });

    group.bench_function("update_importance", |b| {
        let mut counter = 0;
        b.iter(|| {
            let update = mindcache_core::core::memory::MemoryUpdate {
                content: None,
                importance: Some(0.3 + (counter % 7) as f32 * 0.1),
                metadata: None,
                ttl_hours: None,
            };
            let id = &memory_ids[counter % memory_ids.len()];
            black_box(memory_manager.update_memory(id, update).unwrap());
            counter += 1;
        })
    });

    group.bench_function("update_metadata", |b| {
        let mut counter = 0;
        b.iter(|| {
            let mut metadata = HashMap::new();
            metadata.insert("updated".to_string(), counter.to_string());

            let update = mindcache_core::core::memory::MemoryUpdate {
                content: None,
                importance: None,
                metadata: Some(metadata),
                ttl_hours: None,
            };
            let id = &memory_ids[counter % memory_ids.len()];
            black_box(memory_manager.update_memory(id, update).unwrap());
            counter += 1;
        })
    });

    group.finish();
}

fn bench_ffi_operations(c: &mut Criterion) {
    use std::ffi::CString;

    let mut group = c.benchmark_group("ffi_operations");

    group.bench_function("ffi_init_destroy", |b| {
        b.iter(|| {
            let handle = mindcache_core::mindcache_init();
            black_box(handle);
            mindcache_core::mindcache_destroy(handle);
        })
    });

    let handle = mindcache_core::mindcache_init();
    let user_id = CString::new("ffi_bench_user").unwrap();
    let session_id = CString::new("ffi_bench_session").unwrap();
    let metadata = CString::new("{}").unwrap();

    group.bench_function("ffi_save", |b| {
        let mut counter = 0;
        b.iter(|| {
            let content = CString::new(format!("FFI benchmark content {}", counter)).unwrap();
            let result = mindcache_core::mindcache_save(
                handle,
                user_id.as_ptr(),
                session_id.as_ptr(),
                content.as_ptr(),
                0.5,
                -1,
                metadata.as_ptr(),
            );
            if !result.is_null() {
                mindcache_core::mindcache_free_string(result);
            }
            counter += 1;
            black_box(result)
        })
    });

    group.bench_function("ffi_recall", |b| {
        let filter = serde_json::json!({
            "user_id": "ffi_bench_user",
            "limit": 10
        });
        let filter_str = filter.to_string();
        let filter_cstring = CString::new(filter_str).unwrap();

        b.iter(|| {
            let result = mindcache_core::mindcache_recall(handle, filter_cstring.as_ptr());
            if !result.is_null() {
                mindcache_core::mindcache_free_string(result);
            }
            black_box(result)
        })
    });

    mindcache_core::mindcache_destroy(handle);
    group.finish();
}

criterion_group!(
    benches,
    bench_memory_save,
    bench_memory_recall,
    bench_full_text_search,
    bench_session_operations,
    bench_batch_operations,
    bench_concurrent_operations,
    bench_data_scaling,
    bench_memory_update_operations,
    bench_ffi_operations
);

criterion_main!(benches);
