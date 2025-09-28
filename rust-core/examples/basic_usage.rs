// examples/basic_usage.rs
//! Basic MindCache usage example
//! 
//! This example demonstrates the core functionality of MindCache:
//! - Creating and configuring a MindCache instance
//! - Saving memories with different importance levels
//! - Querying and filtering memories
//! - Working with sessions
//! - Basic memory management

use mindcache_core::*;
use mindcache_core::database::models::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧠 MindCache Basic Usage Example");
    println!("=================================\n");

    // Step 1: Create and configure MindCache
    println!("1. Setting up MindCache...");
    
    let config = MindCacheConfig {
        database_path: "./examples/basic_example.db".to_string(),
        default_memory_ttl_hours: Some(720), // 30 days
        enable_compression: true,
        max_memories_per_user: 1000,
        importance_threshold: 0.3,
        enable_request_limits: false, // Disable for example
        ..Default::default()
    };

    // Validate configuration
    config.validate()?;
    println!("✅ Configuration validated");

    // Create database configuration
    let db_config = DatabaseConfig {
        path: config.database_path.clone(),
        enable_wal: true,
        cache_size: -2000, // 2MB cache
        ..Default::default()
    };

    // Initialize database
    let database = Database::new(db_config)?;
    println!("✅ Database initialized");

    // Create validator and managers
    let validator = core::RequestValidator::new(&config);
    let memory_manager = core::memory::MemoryManager::new(database.clone(), validator.clone());
    let session_manager = core::session::SessionManager::new(database.clone(), validator.clone());

    println!("✅ MindCache initialized successfully\n");

    // Step 2: Create a session for organizing memories
    println!("2. Creating a session...");
    
    let user_id = "demo_user";
    let session_id = session_manager.create_session(user_id, Some("Trading Journal".to_string()))?;
    println!("✅ Created session: {}\n", session_id);

    // Step 3: Save some memories with different characteristics
    println!("3. Saving memories...");
    
    let memories_to_save = vec![
        (
            "Bought 100 shares of AAPL at $175.50. Strong technical breakout pattern.",
            0.9, // High importance
            Some(vec!["trading".to_string(), "AAPL".to_string()]),
            {
                let mut meta = HashMap::new();
                meta.insert("symbol".to_string(), "AAPL".to_string());
                meta.insert("action".to_string(), "buy".to_string());
                meta.insert("quantity".to_string(), "100".to_string());
                meta.insert("price".to_string(), "175.50".to_string());
                meta
            }
        ),
        (
            "Fed meeting tomorrow at 2 PM EST. Expecting dovish tone to continue.",
            0.8, // High importance
            Some(vec!["macro".to_string(), "fed".to_string()]),
            {
                let mut meta = HashMap::new();
                meta.insert("event_type".to_string(), "fed_meeting".to_string());
                meta.insert("impact".to_string(), "high".to_string());
                meta
            }
        ),
        (
            "Tesla earnings beat expectations. Stock up 8% in after-hours trading.",
            0.7, // Medium-high importance
            Some(vec!["earnings".to_string(), "TSLA".to_string()]),
            {
                let mut meta = HashMap::new();
                meta.insert("symbol".to_string(), "TSLA".to_string());
                meta.insert("event_type".to_string(), "earnings".to_string());
                meta
            }
        ),
        (
            "Coffee was particularly good today. Barista recommended new blend.",
            0.2, // Low importance
            Some(vec!["personal".to_string(), "coffee".to_string()]),
            HashMap::new()
        ),
        (
            "Market showing signs of consolidation. Volume declining.",
            0.5, // Medium importance
            Some(vec!["analysis".to_string(), "market".to_string()]),
            {
                let mut meta = HashMap::new();
                meta.insert("pattern".to_string(), "consolidation".to_string());
                meta
            }
        ),
    ];

    let mut saved_memory_ids = Vec::new();
    
    for (content, importance, tags, metadata) in memories_to_save {
        let memory = MemoryItem {
            user_id: user_id.to_string(),
            session_id: session_id.clone(),
            content: content.to_string(),
            importance,
            ttl_hours: Some(168), // 1 week
            metadata,
            tags: tags.unwrap_or_default(),
            ..Default::default()
        };
        
        let memory_id = memory_manager.save_memory(memory)?;
        saved_memory_ids.push(memory_id.clone());
        println!("💾 Saved memory: {} (ID: {})", content, memory_id);
    }
    
    println!("✅ Saved {} memories\n", saved_memory_ids.len());

    // Step 4: Query memories with different filters
    println!("4. Querying memories...");
    
    // Get all memories for the user
    println!("\n📋 All memories for user:");
    let all_memories = memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        limit: Some(10),
        ..Default::default()
    })?;
    
    for memory in &all_memories.data {
        println!("  • {} (importance: {:.1})", memory.content, memory.importance);
    }
    
    // Get high-importance memories only
    println!("\n⭐ High-importance memories (>= 0.7):");
    let important_memories = memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        min_importance: Some(0.7),
        ..Default::default()
    })?;
    
    for memory in &important_memories.data {
        println!("  • {} (importance: {:.1})", memory.content, memory.importance);
    }
    
    // Search for specific keywords
    println!("\n🔍 Memories about 'AAPL':");
    let aapl_memories = memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        keywords: Some(vec!["AAPL".to_string()]),
        ..Default::default()
    })?;
    
    for memory in &aapl_memories.data {
        println!("  • {} (importance: {:.1})", memory.content, memory.importance);
        if !memory.metadata.is_empty() {
            println!("    Metadata: {:?}", memory.metadata);
        }
    }

    // Step 5: Demonstrate memory updates
    println!("\n5. Updating a memory...");
    
    if let Some(memory_id) = saved_memory_ids.first() {
        // Get the original memory
        if let Some(original) = memory_manager.get_memory(memory_id)? {
            println!("📝 Original: {}", original.content);
            
            // Update with additional information
            let updated_content = format!("{} UPDATED: Set stop loss at $170.", original.content);
            let update = core::memory::MemoryUpdate {
                content: Some(updated_content.clone()),
                importance: Some(0.95), // Increase importance
                metadata: None,
                ttl_hours: None,
            };
            
            let success = memory_manager.update_memory(memory_id, update)?;
            if success {
                println!("✅ Updated: {}", updated_content);
                
                // Verify the update
                if let Some(updated) = memory_manager.get_memory(memory_id)? {
                    println!("📄 Verified importance: {:.2}", updated.importance);
                }
            }
        }
    }

    // Step 6: Demonstrate pagination
    println!("\n6. Demonstrating pagination...");
    
    // Get first page (2 items)
    let page1 = memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        limit: Some(2),
        offset: Some(0),
        ..Default::default()
    })?;
    
    println!("📄 Page 1 ({} of {} total):", page1.data.len(), page1.total_count);
    for memory in &page1.data {
        println!("  • {}", memory.content.chars().take(50).collect::<String>() + "...");
    }
    
    // Get second page
    let page2 = memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        limit: Some(2),
        offset: Some(2),
        ..Default::default()
    })?;
    
    println!("📄 Page 2 ({} items):", page2.data.len());
    for memory in &page2.data {
        println!("  • {}", memory.content.chars().take(50).collect::<String>() + "...");
    }

    // Step 7: Session summary
    println!("\n7. Generating session summary...");
    
    let sessions = session_manager.get_user_sessions(user_id, Some(10), Some(0))?;
    if let Some(session) = sessions.data.first() {
        println!("📊 Session: {}", session.name.as_ref().unwrap_or(&"Unnamed".to_string()));
        println!("   ID: {}", session.id);
        println!("   Memories: {}", session.memory_count);
        println!("   Created: {}", session.created_at.format("%Y-%m-%d %H:%M:%S"));
        
        // Try to generate a summary
        match session_manager.generate_session_summary(&session.id) {
            Ok(summary) => {
                println!("   Summary: {}", summary.summary_text);
                println!("   Key topics: {:?}", summary.key_topics);
                println!("   Avg importance: {:.2}", summary.importance_score);
            }
            Err(_) => {
                println!("   Summary: (Not enough content for summary)");
            }
        }
    }

    // Step 8: Export data
    println!("\n8. Exporting user data...");
    
    let exported_data = memory_manager.export_user_memories(user_id)?;
    let memory_count = serde_json::from_str::<Vec<MemoryItem>>(&exported_data)?.len();
    println!("📤 Exported {} memories", memory_count);
    
    // Save to file for inspection
    std::fs::write("./examples/exported_memories.json", &exported_data)?;
    println!("💾 Saved export to ./examples/exported_memories.json");

    // Step 9: Memory statistics
    println!("\n9. Memory statistics...");
    
    let user_stats = memory_manager.get_user_memory_stats(user_id)?;
    println!("📈 User Statistics:");
    println!("   Total memories: {}", user_stats.total_memories);
    println!("   Average importance: {:.2}", user_stats.avg_importance);
    println!("   Importance distribution: {:?}", user_stats.importance_distribution);
    println!("   Age distribution: {:?}", user_stats.age_distribution);
    
    if let Some(oldest) = user_stats.oldest_memory {
        println!("   Oldest memory: {}", oldest.format("%Y-%m-%d %H:%M:%S"));
    }

    // Step 10: Cleanup demonstration
    println!("\n10. Cleanup...");
    
    // Delete a low-importance memory
    if let Some(memory_to_delete) = saved_memory_ids.last() {
        let deleted = memory_manager.delete_memory(memory_to_delete)?;
        if deleted {
            println!("🗑️  Deleted memory: {}", memory_to_delete);
        }
    }
    
    // Show final count
    let final_memories = memory_manager.recall_memories(QueryFilter {
        user_id: Some(user_id.to_string()),
        ..Default::default()
    })?;
    println!("📊 Final memory count: {}", final_memories.total_count);

    println!("\n🎉 Example completed successfully!");
    println!("Database saved to: {}", config.database_path);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_example_components() {
        // Test that the example components work
        let config = MindCacheConfig::default();
        assert!(config.validate().is_ok());
        
        let memory = MemoryItem::default();
        assert_eq!(memory.importance, 0.5);
    }
}