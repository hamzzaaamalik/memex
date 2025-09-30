//! Property-based tests using proptest

use memex_core::core::memory::MemoryManager;
use memex_core::core::{MemexConfig, RequestValidator};
use memex_core::database::models::*;
use memex_core::database::{Database, DatabaseConfig};
use proptest::prelude::*;
use std::collections::HashMap;
use tempfile::TempDir;

prop_compose! {
    fn arb_memory_item()
        (user_id in "[a-zA-Z0-9_]{1,50}",
         session_id in "[a-zA-Z0-9_]{1,50}",
         content in ".*{1,1000}",
         importance in 0.0f32..1.0f32,
         ttl_hours in prop::option::of(1u32..8760u32))
        -> MemoryItem {
        MemoryItem {
            id: String::new(),
            user_id,
            session_id,
            content,
            importance,
            ttl_hours,
            metadata: HashMap::new(),
            ..Default::default()
        }
    }
}

prop_compose! {
    fn arb_query_filter()
        (user_id in prop::option::of("[a-zA-Z0-9_]{1,50}"),
         session_id in prop::option::of("[a-zA-Z0-9_]{1,50}"),
         keywords in prop::option::of(prop::collection::vec("\\w+", 0..5)),
         limit in prop::option::of(1usize..1000usize),
         offset in prop::option::of(0usize..10000usize),
         min_importance in prop::option::of(0.0f32..1.0f32))
        -> QueryFilter {
        QueryFilter {
            user_id,
            session_id,
            keywords,
            date_from: None,
            date_to: None,
            limit,
            offset,
            min_importance,
        }
    }
}

fn setup_property_test_env() -> (MemoryManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_config = DatabaseConfig {
        path: temp_dir
            .path()
            .join("property_test.db")
            .to_string_lossy()
            .to_string(),
        ..Default::default()
    };

    let database = Database::new(db_config).unwrap();
    let config = MemexConfig {
        enable_request_limits: false,
        ..Default::default()
    };
    let validator = RequestValidator::new(&config);
    let memory_manager = MemoryManager::new(database, validator);

    (memory_manager, temp_dir)
}

proptest! {
    #[test]
    fn test_memory_save_recall_roundtrip(memory in arb_memory_item()) {
        let (memory_manager, _temp_dir) = setup_property_test_env();

        // Save the memory
        let memory_id = memory_manager.save_memory(memory.clone()).unwrap();

        // Recall by user should find the memory
        let filter = QueryFilter {
            user_id: Some(memory.user_id.clone()),
            ..Default::default()
        };

        let recalled = memory_manager.recall_memories(filter).unwrap();

        // Should find at least the saved memory
        prop_assert!(recalled.data.len() >= 1);

        // Find our specific memory
        let found_memory = recalled.data.iter()
            .find(|m| m.id == memory_id)
            .unwrap();

        // Verify key properties are preserved
        prop_assert_eq!(&found_memory.user_id, &memory.user_id);
        prop_assert_eq!(&found_memory.session_id, &memory.session_id);
        prop_assert_eq!(&found_memory.content, &memory.content);
        prop_assert!((found_memory.importance - memory.importance).abs() < 0.001);
    }

    #[test]
    fn test_recall_filter_properties(
        memories in prop::collection::vec(arb_memory_item(), 1..20),
        filter in arb_query_filter()
    ) {
        let (memory_manager, _temp_dir) = setup_property_test_env();

        // Save all memories
        for memory in &memories {
            let _ = memory_manager.save_memory(memory.clone());
        }

        // Apply filter
        let result = memory_manager.recall_memories(filter.clone()).unwrap();

        // Verify filter constraints are respected
        for memory in &result.data {
            if let Some(ref user_id) = filter.user_id {
                prop_assert_eq!(&memory.user_id, user_id);
            }

            if let Some(ref session_id) = filter.session_id {
                prop_assert_eq!(&memory.session_id, session_id);
            }

            if let Some(min_importance) = filter.min_importance {
                prop_assert!(memory.importance >= min_importance);
            }
        }

        // Verify limit is respected
        if let Some(limit) = filter.limit {
            prop_assert!(result.data.len() <= limit);
        }

        // Verify pagination consistency
        prop_assert_eq!(result.data.len(), result.per_page.min(result.total_count as usize - result.page * result.per_page));
    }

    #[test]
    fn test_importance_bounds(
        user_id in "[a-zA-Z0-9_]{1,50}",
        session_id in "[a-zA-Z0-9_]{1,50}",
        content in ".*{1,100}",
        importance in any::<f32>()
    ) {
        let (memory_manager, _temp_dir) = setup_property_test_env();

        let memory = MemoryItem {
            user_id,
            session_id,
            content,
            importance,
            ..Default::default()
        };

        // Save should either succeed (with clamped importance) or fail gracefully
        match memory_manager.save_memory(memory) {
            Ok(memory_id) => {
                // If save succeeded, retrieve and verify importance is clamped
                let retrieved = memory_manager.get_memory(&memory_id).unwrap().unwrap();
                prop_assert!(retrieved.importance >= 0.0);
                prop_assert!(retrieved.importance <= 1.0);
            }
            Err(_) => {
                // Save failed, which is acceptable for invalid input
            }
        }
    }

    #[test]
    fn test_content_preservation(
        user_id in "[a-zA-Z0-9_]{1,50}",
        session_id in "[a-zA-Z0-9_]{1,50}",
        content in ".*{0,10000}" // Including empty content
    ) {
        let (memory_manager, _temp_dir) = setup_property_test_env();

        let memory = MemoryItem {
            user_id,
            session_id,
            content: content.clone(),
            importance: 0.5,
            ..Default::default()
        };

        match memory_manager.save_memory(memory) {
            Ok(memory_id) => {
                let retrieved = memory_manager.get_memory(&memory_id).unwrap().unwrap();
                // Content should be preserved exactly (if save succeeded)
                prop_assert_eq!(retrieved.content, content);
            }
            Err(_) => {
                // Some content might be invalid (e.g., empty), which is fine
            }
        }
    }

    #[test]
    fn test_search_consistency(
        memories in prop::collection::vec(arb_memory_item(), 5..50),
        search_term in "\\w+"
    ) {
        let (memory_manager, _temp_dir) = setup_property_test_env();

        // Filter to single user for consistency
        let user_id = "consistent_user";
        let session_id = "consistent_session";

        let mut saved_memories = Vec::new();
        for mut memory in memories {
            memory.user_id = user_id.to_string();
            memory.session_id = session_id.to_string();

            if memory_manager.save_memory(memory.clone()).is_ok() {
                saved_memories.push(memory);
            }
        }

        // Search for the term
        let search_results = memory_manager
            .search_memories(user_id, &search_term, Some(100), Some(0))
            .unwrap();

        // Every result should contain the search term (case-insensitive)
        for result in &search_results.data {
            prop_assert!(result.content.to_lowercase().contains(&search_term.to_lowercase()));
        }

        // Search should find all memories containing the term
        let expected_matches: Vec<_> = saved_memories.iter()
            .filter(|m| m.content.to_lowercase().contains(&search_term.to_lowercase()))
            .collect();

        prop_assert_eq!(search_results.data.len(), expected_matches.len());
    }

    #[test]
    fn test_pagination_consistency(
        memories in prop::collection::vec(arb_memory_item(), 20..100),
        page_size in 1usize..50usize
    ) {
        let (memory_manager, _temp_dir) = setup_property_test_env();

        let user_id = "pagination_user";

        // Save all memories for the same user
        let mut saved_count = 0;
        for mut memory in memories {
            memory.user_id = user_id.to_string();
            if memory_manager.save_memory(memory).is_ok() {
                saved_count += 1;
            }
        }

        // Test pagination
        let mut total_retrieved = 0;
        let mut page = 0;
        let mut all_ids = std::collections::HashSet::new();

        loop {
            let filter = QueryFilter {
                user_id: Some(user_id.to_string()),
                limit: Some(page_size),
                offset: Some(page * page_size),
                ..Default::default()
            };

            let result = memory_manager.recall_memories(filter).unwrap();

            if result.data.is_empty() {
                break;
            }

            // Verify no duplicates across pages
            for memory in &result.data {
                prop_assert!(all_ids.insert(memory.id.clone()), "Duplicate memory across pages");
            }

            total_retrieved += result.data.len();

            // Verify pagination metadata
            prop_assert_eq!(result.page, page);
            prop_assert_eq!(result.per_page, page_size);
            prop_assert_eq!(result.total_count as usize, saved_count);

            if !result.has_next {
                break;
            }

            page += 1;

            // Safety check to prevent infinite loops
            prop_assert!(page < 1000, "Too many pages - possible infinite loop");
        }

        // Should have retrieved all saved memories
        prop_assert_eq!(total_retrieved, saved_count);
    }

    #[test]
    fn test_memory_ordering(
        memories in prop::collection::vec(arb_memory_item(), 10..50)
    ) {
        let (memory_manager, _temp_dir) = setup_property_test_env();

        let user_id = "ordering_user";

        // Save memories with known timestamps
        let mut saved_memories = Vec::new();
        for mut memory in memories {
            memory.user_id = user_id.to_string();

            if memory_manager.save_memory(memory.clone()).is_ok() {
                saved_memories.push(memory);
            }

            // Small delay to ensure different timestamps
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        // Recall all memories
        let filter = QueryFilter {
            user_id: Some(user_id.to_string()),
            limit: Some(saved_memories.len()),
            ..Default::default()
        };

        let result = memory_manager.recall_memories(filter).unwrap();

        // Verify ordering (should be by creation time descending, then importance descending)
        for i in 1..result.data.len() {
            let prev = &result.data[i - 1];
            let curr = &result.data[i];

            // Either created later, or created at same time but higher importance
            prop_assert!(
                prev.created_at >= curr.created_at,
                "Memories should be ordered by creation time descending"
            );

            if prev.created_at == curr.created_at {
                prop_assert!(
                    prev.importance >= curr.importance,
                    "Within same timestamp, should be ordered by importance descending"
                );
            }
        }
    }
}

// Additional focused property tests for edge cases

proptest! {
    #[test]
    fn test_unicode_content_handling(
        user_id in "[a-zA-Z0-9_]{1,20}",
        session_id in "[a-zA-Z0-9_]{1,20}",
        content in ".*{1,500}" // May contain Unicode
    ) {
        let (memory_manager, _temp_dir) = setup_property_test_env();

        let memory = MemoryItem {
            user_id: user_id.clone(),
            session_id,
            content: content.clone(),
            importance: 0.5,
            ..Default::default()
        };

        if let Ok(memory_id) = memory_manager.save_memory(memory) {
            let retrieved = memory_manager.get_memory(&memory_id).unwrap().unwrap();

            // Unicode content should be preserved exactly
            prop_assert_eq!(retrieved.content, content);

            // Should be findable by search if content is non-empty
            if !content.trim().is_empty() {
                // Extract a word from content for searching
                if let Some(word) = content.split_whitespace().next() {
                    if word.len() >= 2 {
                        let search_results = memory_manager
                            .search_memories(&user_id, word, Some(10), Some(0))
                            .unwrap();

                        // Should find the memory in results
                        prop_assert!(!search_results.data.is_empty());
                    }
                }
            }
        }
    }
}
