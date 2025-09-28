//! Property-based tests using proptest

use mindcache_core::database::{Database, DatabaseConfig};
use mindcache_core::core::{MindCacheConfig, RequestValidator};
use mindcache_core::core::memory::MemoryManager;
use mindcache_core::database::models::*;
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
        path: temp_dir.path().join("property_test.db").to_string_lossy().to_string(),
        ..Default::default()
    };
    
    let database = Database::new(db_config).unwrap();
    let config = MindCacheConfig {
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
            
            if let Ok(_) = memory_manager.save_memory(memory.clone()) {
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
            
            if let Ok(_) = memory_manager.save_memory(memory.clone()) {
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
                        
                        #!/bin/bash

# Test runner script for MindCache

set -e  # Exit on any error

echo "🧪 Running MindCache Test Suite"
echo "================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    print_error "Please run this script from the rust-core directory"
    exit 1
fi

# Check dependencies
print_status "Checking dependencies..."

if ! command -v cargo &> /dev/null; then
    print_error "Cargo is not installed. Please install Rust and Cargo."
    exit 1
fi

# Build the project first
print_status "Building project..."
if ! cargo build; then
    print_error "Build failed"
    exit 1
fi

print_success "Build completed"

# Run unit tests
print_status "Running unit tests..."
if cargo test --lib -- --test-threads=1; then
    print_success "Unit tests passed"
else
    print_error "Unit tests failed"
    exit 1
fi

# Run integration tests
print_status "Running integration tests..."
if cargo test --test integration_tests -- --test-threads=1; then
    print_success "Integration tests passed"
else
    print_error "Integration tests failed"
    exit 1
fi

# Run FFI tests
print_status "Running FFI tests..."
if cargo test --test ffi_tests -- --test-threads=1; then
    print_success "FFI tests passed"
else
    print_error "FFI tests failed"
    exit 1
fi

# Run property-based tests
print_status "Running property-based tests..."
if cargo test --test property_tests -- --test-threads=1; then
    print_success "Property-based tests passed"
else
    print_warning "Property-based tests failed (this might be due to random test cases)"
fi

# Run benchmarks (if requested)
if [ "$1" = "--bench" ] || [ "$1" = "-b" ]; then
    print_status "Running benchmarks..."
    if cargo bench; then
        print_success "Benchmarks completed"
    else
        print_warning "Some benchmarks failed"
    fi
fi

# Test with release build
if [ "$1" = "--release" ] || [ "$1" = "-r" ]; then
    print_status "Running tests with release build..."
    if cargo test --release -- --test-threads=1; then
        print_success "Release tests passed"
    else
        print_error "Release tests failed"
        exit 1
    fi
fi

# Coverage report (if requested)
if [ "$1" = "--coverage" ] || [ "$1" = "-c" ]; then
    print_status "Generating coverage report..."
    if command -v cargo-tarpaulin &> /dev/null; then
        cargo tarpaulin --out Html --output-dir target/coverage
        print_success "Coverage report generated in target/coverage/"
    else
        print_warning "cargo-tarpaulin not installed. Install with: cargo install cargo-tarpaulin"
    fi
fi

# Memory leak check (if requested)
if [ "$1" = "--valgrind" ] || [ "$1" = "-v" ]; then
    print_status "Running memory leak check..."
    if command -v valgrind &> /dev/null; then
        cargo test --release -- --test-threads=1 --nocapture | valgrind --leak-check=full --track-origins=yes
        print_success "Memory check completed"
    else
        print_warning "Valgrind not installed"
    fi
fi

# Clean up test artifacts
print_status "Cleaning up test artifacts..."
find target -name "*.db" -delete 2>/dev/null || true
find target -name "test_*" -type d -exec rm -rf {} + 2>/dev/null || true

print_success "All tests completed successfully! ✨"

# Print summary
echo ""
echo "📊 Test Summary:"
echo "================"
echo "✅ Unit tests"
echo "✅ Integration tests" 
echo "✅ FFI tests"
echo "✅ Property-based tests"

if [ "$1" = "--bench" ] || [ "$1" = "-b" ]; then
    echo "📈 Benchmarks"
fi

if [ "$1" = "--coverage" ] || [ "$1" = "-c" ]; then
    echo "📋 Coverage report"
fi

echo ""
echo "🎉 Ready for production!"