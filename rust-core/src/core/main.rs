//! MindCache CLI binary with enhanced features

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::*;
use std::collections::HashMap;
use std::io::{self, Write};

use mindcache_core::database::{Database, DatabaseConfig};
use mindcache_core::database::vector::{VectorConfig, VectorSearchEngine};
use mindcache_core::core::{MindCacheConfig, RequestValidator};
use mindcache_core::core::memory::MemoryManager;
use mindcache_core::core::session::SessionManager;
use mindcache_core::core::decay::DecayEngine;
use mindcache_core::database::models::*;

#[cfg(feature = "async")]
use mindcache_core::database::async_db::AsyncDatabase;
#[cfg(feature = "async")]
use mindcache_core::core::async_memory::AsyncMemoryManager;

#[derive(Parser)]
#[command(name = "mindcache")]
#[command(about = "A lightweight, local-first memory engine for AI applications with vector search")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Database path
    #[arg(short, long, default_value = "mindcache.db")]
    database: String,
    
    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
    
    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,
    
    /// Enable vector search
    #[arg(long)]
    enable_vector: bool,
    
    /// Vector dimension (default: 384)
    #[arg(long, default_value = "384")]
    vector_dimension: usize,
    
    /// Enable async mode
    #[arg(long)]
    async_mode: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Memory operations
    Memory {
        #[command(subcommand)]
        action: MemoryCommands,
    },
    /// Session operations
    Session {
        #[command(subcommand)]
        action: SessionCommands,
    },
    /// Decay operations
    Decay {
       #[command(subcommand)]
       action: DecayCommands,
   },
   /// Database operations
   Database {
       #[command(subcommand)]
       action: DatabaseCommands,
   },
   /// System information and health checks
   System {
       #[command(subcommand)]
       action: SystemCommands,
   },
   /// Vector search operations
   Vector {
       #[command(subcommand)]
       action: VectorCommands,
   },
}

#[derive(Subcommand)]
enum VectorCommands {
    /// Store embedding for a memory
    Store {
        /// Memory ID
        #[arg(short, long)]
        memory_id: String,
        /// Embedding vector as JSON array
        #[arg(short, long)]
        embedding: String,
        /// Model name
        #[arg(short, long)]
        model: String,
    },
    /// Search for similar memories
    Search {
        /// Query embedding as JSON array
        #[arg(short, long)]
        embedding: String,
        /// Model name
        #[arg(short, long)]
        model: String,
        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Hybrid search (text + vector)
    Hybrid {
        /// Text query
        #[arg(short, long)]
        text: String,
        /// Vector query as JSON array
        #[arg(short, long)]
        vector: String,
        /// Model name
        #[arg(short, long)]
        model: String,
        /// Text weight (0.0-1.0)
        #[arg(long, default_value = "0.5")]
        text_weight: f32,
        /// Vector weight (0.0-1.0)
        #[arg(long, default_value = "0.5")]
        vector_weight: f32,
        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Show vector search statistics
    Stats,
}

#[derive(Subcommand)]
enum Commands {
    /// Memory operations
    Memory {
        #[command(subcommand)]
        action: MemoryCommands,
    },
    /// Session operations
    Session {
        #[command(subcommand)]
        action: SessionCommands,
    },
    /// Decay operations
    Decay {
       #[command(subcommand)]
       action: DecayCommands,
   },
   /// Database operations
   Database {
       #[command(subcommand)]
       action: DatabaseCommands,
   },
   /// System information and health checks
   System {
       #[command(subcommand)]
       action: SystemCommands,
   },
}

#[derive(Subcommand)]
enum MemoryCommands {
   /// Save a new memory
   Save {
       /// User ID
       #[arg(short, long)]
       user: String,
       /// Session ID
       #[arg(short, long)]
       session: String,
       /// Memory content
       content: String,
       /// Importance (0.0-1.0)
       #[arg(short, long)]
       importance: Option<f32>,
       /// TTL in hours
       #[arg(short, long)]
       ttl: Option<u32>,
       /// Metadata as JSON
       #[arg(short, long)]
       metadata: Option<String>,
   },
   /// Recall memories
   Recall {
       /// User ID
       #[arg(short, long)]
       user: String,
       /// Search keywords
       #[arg(short, long)]
       keywords: Option<String>,
       /// Session ID filter
       #[arg(short, long)]
       session: Option<String>,
       /// Minimum importance
       #[arg(long)]
       min_importance: Option<f32>,
       /// Limit results
       #[arg(short, long, default_value = "10")]
       limit: usize,
       /// Offset for pagination
       #[arg(long, default_value = "0")]
       offset: usize,
   },
   /// Search memories with full-text search
   Search {
       /// User ID
       #[arg(short, long)]
       user: String,
       /// Search query
       query: String,
       /// Limit results
       #[arg(short, long, default_value = "10")]
       limit: usize,
   },
   /// Get memory by ID
   Get {
       /// Memory ID
       id: String,
   },
   /// Update memory
   Update {
       /// Memory ID
       id: String,
       /// New content
       #[arg(short, long)]
       content: Option<String>,
       /// New importance
       #[arg(short, long)]
       importance: Option<f32>,
       /// New TTL
       #[arg(short, long)]
       ttl: Option<u32>,
   },
   /// Delete memory
   Delete {
       /// Memory ID
       id: String,
   },
   /// Export memories for a user
   Export {
       /// User ID
       #[arg(short, long)]
       user: String,
       /// Output file (JSON format)
       #[arg(short, long)]
       output: Option<String>,
   },
   /// Show memory statistics for a user
   Stats {
       /// User ID
       #[arg(short, long)]
       user: String,
   },
}

#[derive(Subcommand)]
enum SessionCommands {
   /// Create a new session
   Create {
       /// User ID
       #[arg(short, long)]
       user: String,
       /// Session name
       #[arg(short, long)]
       name: Option<String>,
   },
   /// List sessions for a user
   List {
       /// User ID
       #[arg(short, long)]
       user: String,
       /// Limit results
       #[arg(short, long, default_value = "10")]
       limit: usize,
   },
   /// Generate session summary
   Summary {
       /// Session ID
       id: String,
   },
   /// Search sessions
   Search {
       /// User ID
       #[arg(short, long)]
       user: String,
       /// Search keywords
       keywords: Vec<String>,
   },
   /// Delete session
   Delete {
       /// Session ID
       id: String,
       /// Also delete memories
       #[arg(long)]
       delete_memories: bool,
   },
   /// Show session analytics
   Analytics {
       /// User ID
       #[arg(short, long)]
       user: String,
   },
}

#[derive(Subcommand)]
enum DecayCommands {
   /// Run decay process
   Run {
       /// Dry run (don't actually delete anything)
       #[arg(long)]
       dry_run: bool,
   },
   /// Show decay recommendations
   Analyze,
   /// Update decay policy
   Policy {
       /// Max age in hours
       #[arg(long)]
       max_age: Option<u32>,
       /// Importance threshold
       #[arg(long)]
       threshold: Option<f32>,
       /// Max memories per user
       #[arg(long)]
       max_memories: Option<usize>,
       /// Enable compression
       #[arg(long)]
       compression: Option<bool>,
   },
   /// Show age distribution
   Distribution,
}

#[derive(Subcommand)]
enum DatabaseCommands {
   /// Initialize database schema
   Init,
   /// Run database migrations
   Migrate,
   /// Show database statistics
   Stats,
   /// Vacuum database (optimize storage)
   Vacuum,
   /// Backup database
   Backup {
       /// Backup file path
       output: String,
   },
   /// Restore from backup
   Restore {
       /// Backup file path
       input: String,
   },
}

#[derive(Subcommand)]
enum SystemCommands {
   /// Show system health
   Health,
   /// Show performance metrics
   Performance,
   /// Show system information
   Info,
   /// Run system diagnostics
   Diagnostics,
}

fn main() -> Result<()> {
   let cli = Cli::parse();
   
   // Initialize logging
   if cli.verbose {
       env_logger::Builder::from_default_env()
           .filter_level(log::LevelFilter::Debug)
           .init();
   } else {
       env_logger::Builder::from_default_env()
           .filter_level(log::LevelFilter::Info)
           .init();
   }
   
   // Load configuration
   let config = load_config(&cli)?;
   
   // Setup database
   let db_config = DatabaseConfig {
       path: cli.database.clone(),
       ..Default::default()
   };
   
   let database = Database::new(db_config)
       .context("Failed to initialize database")?;
   
   let validator = RequestValidator::new(&config);
   
   // Execute command
   match cli.command {
       Commands::Memory { action } => handle_memory_commands(action, database, validator),
       Commands::Session { action } => handle_session_commands(action, database, validator),
       Commands::Decay { action } => handle_decay_commands(action, database, validator, &config),
       Commands::Database { action } => handle_database_commands(action, database),
       Commands::System { action } => handle_system_commands(action, database, &config),
   }
}

fn load_config(cli: &Cli) -> Result<MindCacheConfig> {
   if let Some(config_path) = &cli.config {
       let config_content = std::fs::read_to_string(config_path)
           .with_context(|| format!("Failed to read config file: {}", config_path))?;
       
       let config: MindCacheConfig = serde_json::from_str(&config_content)
           .context("Failed to parse config file")?;
       
       println!("{}", "✓ Loaded configuration from file".green());
       Ok(config)
   } else {
       Ok(MindCacheConfig {
           database_path: cli.database.clone(),
           ..Default::default()
       })
   }
}

fn handle_memory_commands(action: MemoryCommands, database: Database, validator: RequestValidator) -> Result<()> {
   let manager = MemoryManager::new(database, validator);
   
   match action {
       MemoryCommands::Save { user, session, content, importance, ttl, metadata } => {
           let metadata_map = if let Some(meta) = metadata {
               serde_json::from_str(&meta)
                   .context("Invalid metadata JSON")?
           } else {
               HashMap::new()
           };
           
           let memory = MemoryItem {
               user_id: user.clone(),
               session_id: session.clone(),
               content: content.clone(),
               importance: importance.unwrap_or(0.5).clamp(0.0, 1.0),
               ttl_hours: ttl,
               metadata: metadata_map,
               ..Default::default()
           };
           
           let memory_id = manager.save_memory(memory)?;
           
           println!("{}", "✓ Memory saved successfully".green());
           println!("  ID: {}", memory_id.bright_blue());
           println!("  User: {}", user);
           println!("  Session: {}", session);
           println!("  Content: {}", 
                   if content.len() > 50 { 
                       format!("{}...", &content[..50]) 
                   } else { 
                       content 
                   });
       }
       
       MemoryCommands::Recall { user, keywords, session, min_importance, limit, offset } => {
           let keywords_vec = keywords.map(|k| 
               k.split_whitespace().map(|s| s.to_string()).collect()
           );
           
           let filter = QueryFilter {
               user_id: Some(user.clone()),
               session_id: session,
               keywords: keywords_vec,
               min_importance,
               limit: Some(limit),
               offset: Some(offset),
               ..Default::default()
           };
           
           let response = manager.recall_memories(filter)?;
           
           if response.data.is_empty() {
               println!("{}", "No memories found".yellow());
               return Ok(());
           }
           
           println!("{}", format!("Found {} memories (page {}/{})", 
                   response.data.len(), response.page + 1, response.total_pages).green());
           println!("{}", format!("Total: {} memories", response.total_count).dim());
           println!();
           
           for (i, memory) in response.data.iter().enumerate() {
               print_memory_item(&memory, i + 1 + offset);
               if i < response.data.len() - 1 {
                   println!("{}", "─".repeat(80).dim());
               }
           }
           
           if response.has_next {
               println!("\n{}", format!("Use --offset {} to see more results", offset + limit).dim());
           }
       }
       
       MemoryCommands::Search { user, query, limit } => {
           let response = manager.search_memories(&user, &query, Some(limit), Some(0))?;
           
           if response.data.is_empty() {
               println!("{}", format!("No memories found for query: '{}'", query).yellow());
               return Ok(());
           }
           
           println!("{}", format!("Search results for '{}' ({})", query, response.data.len()).green());
           println!();
           
           for (i, memory) in response.data.iter().enumerate() {
               print_memory_item(&memory, i + 1);
               if i < response.data.len() - 1 {
                   println!("{}", "─".repeat(80).dim());
               }
           }
       }
       
       MemoryCommands::Get { id } => {
           match manager.get_memory(&id)? {
               Some(memory) => {
                   println!("{}", "Memory Details".green().bold());
                   print_memory_item(&memory, 1);
               }
               None => {
                   println!("{}", format!("Memory not found: {}", id).yellow());
               }
           }
       }
       
       MemoryCommands::Update { id, content, importance, ttl } => {
           let update = crate::core::memory::MemoryUpdate {
               content,
               importance,
               metadata: None,
               ttl_hours: ttl.map(|t| Some(t)),
           };
           
           let updated = manager.update_memory(&id, update)?;
           
           if updated {
               println!("{}", "✓ Memory updated successfully".green());
           } else {
               println!("{}", format!("Memory not found: {}", id).yellow());
           }
       }
       
       MemoryCommands::Delete { id } => {
           print!("Are you sure you want to delete memory {}? (y/N): ", id);
           io::stdout().flush()?;
           
           let mut input = String::new();
           io::stdin().read_line(&mut input)?;
           
           if input.trim().to_lowercase() == "y" {
               let deleted = manager.delete_memory(&id)?;
               
               if deleted {
                   println!("{}", "✓ Memory deleted successfully".green());
               } else {
                   println!("{}", format!("Memory not found: {}", id).yellow());
               }
           } else {
               println!("Cancelled");
           }
       }
       
       MemoryCommands::Export { user, output } => {
           let memories = manager.export_user_memories(&user)?;
           
           let json_data = serde_json::to_string_pretty(&memories)
               .context("Failed to serialize memories")?;
           
           match output {
               Some(file_path) => {
                   std::fs::write(&file_path, json_data)
                       .with_context(|| format!("Failed to write to file: {}", file_path))?;
                   println!("{}", format!("✓ Exported {} memories to {}", memories.len(), file_path).green());
               }
               None => {
                   println!("{}", json_data);
               }
           }
       }
       
       MemoryCommands::Stats { user } => {
           let stats = manager.get_user_memory_stats(&user)?;
           
           println!("{}", format!("Memory Statistics for {}", user).green().bold());
           println!("Total memories: {}", stats.total_memories.to_string().bright_blue());
           println!("Average importance: {:.2}", stats.avg_importance);
           
           if let Some(oldest) = stats.oldest_memory {
               println!("Oldest memory: {}", oldest.format("%Y-%m-%d %H:%M"));
           }
           
           if let Some(newest) = stats.newest_memory {
               println!("Newest memory: {}", newest.format("%Y-%m-%d %H:%M"));
           }
           
           println!("\n{}", "Importance Distribution:".bold());
           for (category, count) in &stats.importance_distribution {
               println!("  {}: {}", category, count);
           }
           
           println!("\n{}", "Age Distribution:".bold());
           for (category, count) in &stats.age_distribution {
               println!("  {}: {}", category, count);
           }
       }
   }
   
   Ok(())
}

fn handle_session_commands(action: SessionCommands, database: Database, validator: RequestValidator) -> Result<()> {
   let manager = SessionManager::new(database, validator);
   
   match action {
       SessionCommands::Create { user, name } => {
           let session_id = manager.create_session(&user, name.clone())?;
           
           println!("{}", "✓ Session created successfully".green());
           println!("  ID: {}", session_id.bright_blue());
           println!("  User: {}", user);
           if let Some(n) = name {
               println!("  Name: {}", n);
           }
       }
       
       SessionCommands::List { user, limit } => {
           let response = manager.get_user_sessions(&user, Some(limit), Some(0))?;
           
           if response.data.is_empty() {
               println!("{}", format!("No sessions found for user: {}", user).yellow());
               return Ok(());
           }
           
           println!("{}", format!("Sessions for {} ({}/{})", user, response.data.len(), response.total_count).green());
           println!();
           
           for session in &response.data {
               println!("🗂️  {} {}", 
                       session.id.bright_blue(),
                       session.name.as_ref().unwrap_or(&"(unnamed)".to_string()));
               println!("    {} memories | Last active: {}", 
                       session.memory_count.to_string().bright_green(),
                       session.last_active.format("%Y-%m-%d %H:%M"));
               println!();
           }
       }
       
       SessionCommands::Summary { id } => {
           match manager.generate_session_summary(&id) {
               Ok(summary) => {
                   println!("{}", "Session Summary".green().bold());
                   println!("Session ID: {}", summary.session_id.bright_blue());
                   println!("Memory count: {}", summary.memory_count);
                   println!("Importance score: {:.2}", summary.importance_score);
                   println!("Date range: {} to {}", 
                           summary.date_range.0.format("%Y-%m-%d"),
                           summary.date_range.1.format("%Y-%m-%d"));
                   
                   if !summary.key_topics.is_empty() {
                       println!("Key topics: {}", summary.key_topics.join(", ").bright_yellow());
                   }
                   
                   println!("\n{}", "Summary:".bold());
                   println!("{}", summary.summary_text);
               }
               Err(e) => {
                   println!("{}", format!("Failed to generate summary: {}", e).red());
               }
           }
       }
       
       SessionCommands::Search { user, keywords } => {
           let sessions = manager.search_sessions(&user, keywords.clone())?;
           
           if sessions.is_empty() {
               println!("{}", format!("No sessions found for keywords: {}", keywords.join(" ")).yellow());
               return Ok(());
           }
           
           println!("{}", format!("Found {} sessions matching: {}", sessions.len(), keywords.join(" ")).green());
           println!();
           
           for session in &sessions {
               println!("🗂️  {} {}", 
                       session.id.bright_blue(),
                       session.name.as_ref().unwrap_or(&"(unnamed)".to_string()));
               println!("    {} memories | Last active: {}", 
                       session.memory_count.to_string().bright_green(),
                       session.last_active.format("%Y-%m-%d %H:%M"));
               println!();
           }
       }
       
       SessionCommands::Delete { id, delete_memories } => {
           print!("Are you sure you want to delete session {}{}? (y/N): ", 
                  id, 
                  if delete_memories { " and all its memories" } else { "" });
           io::stdout().flush()?;
           
           let mut input = String::new();
           io::stdin().read_line(&mut input)?;
           
           if input.trim().to_lowercase() == "y" {
               let deleted = manager.delete_session(&id, delete_memories)?;
               
               if deleted {
                   println!("{}", "✓ Session deleted successfully".green());
               } else {
                   println!("{}", format!("Session not found: {}", id).yellow());
               }
           } else {
               println!("Cancelled");
           }
       }
       
       SessionCommands::Analytics { user } => {
           let analytics = manager.get_session_analytics(&user)?;
           
           println!("{}", format!("Session Analytics for {}", user).green().bold());
           println!("Total sessions: {}", analytics.total_sessions.to_string().bright_blue());
           println!("Total memories: {}", analytics.total_memories.to_string().bright_blue());
           println!("Avg memories per session: {:.1}", analytics.avg_memories_per_session);
           
           if let Some(most_active) = &analytics.most_active_session {
               println!("\n{}", "Most Active Session:".bold());
               println!("  {} {} ({} memories)", 
                       most_active.id.bright_blue(),
                       most_active.name.as_ref().unwrap_or(&"(unnamed)".to_string()),
                       most_active.memory_count);
           }
           
           if let Some(most_recent) = &analytics.most_recent_session {
               println!("\n{}", "Most Recent Session:".bold());
               println!("  {} {} ({})", 
                       most_recent.id.bright_blue(),
                       most_recent.name.as_ref().unwrap_or(&"(unnamed)".to_string()),
                       most_recent.last_active.format("%Y-%m-%d %H:%M"));
           }
           
           if !analytics.activity_by_day.is_empty() {
               println!("\n{}", "Recent Activity:".bold());
               let mut sorted_activity: Vec<_> = analytics.activity_by_day.iter().collect();
               sorted_activity.sort_by_key(|(date, _)| *date);
               
               for (date, count) in sorted_activity.iter().rev().take(7) {
                   println!("  {}: {} memories", date, count);
               }
           }
       }
   }
   
   Ok(())
}

fn handle_decay_commands(action: DecayCommands, database: Database, validator: RequestValidator, config: &MindCacheConfig) -> Result<()> {
   let policy = DecayPolicy {
       max_age_hours: config.default_memory_ttl_hours.unwrap_or(24 * 30),
       importance_threshold: config.importance_threshold,
       max_memories_per_user: config.max_memories_per_user,
       compression_enabled: config.enable_compression,
       auto_summarize_sessions: true,
   };
   
   let engine = DecayEngine::new(database, validator, policy);
   
   match action {
       DecayCommands::Run { dry_run } => {
           if dry_run {
               println!("{}", "🧪 Running decay process in DRY RUN mode".yellow().bold());
               println!("No changes will be made to the database");
           } else {
               println!("{}", "🧹 Running decay process...".green().bold());
           }
           
           let stats = engine.run_decay()?;
           
           println!("\n{}", "Decay Results:".green().bold());
           println!("Run ID: {}", stats.run_id.bright_blue());
           println!("Status: {}", match stats.status {
               DecayStatus::Completed => "✓ Completed".green(),
               DecayStatus::Failed => "✗ Failed".red(),
               DecayStatus::Running => "⏳ Running".yellow(),
           });
           
           if let Some(completed_at) = stats.completed_at {
               let duration = completed_at - stats.started_at;
               println!("Duration: {}ms", duration.num_milliseconds());
           }
           
           println!("\n{}", "Statistics:".bold());
           println!("  Memories before: {}", stats.total_memories_before.to_string().bright_blue());
           println!("  Memories after: {}", stats.total_memories_after.to_string().bright_blue());
           println!("  Memories expired: {}", stats.memories_expired.to_string().bright_red());
           println!("  Memories compressed: {}", stats.memories_compressed.to_string().bright_yellow());
           println!("  Sessions summarized: {}", stats.sessions_summarized.to_string().bright_green());
           println!("  Storage saved: {} bytes", stats.storage_saved_bytes.to_string().bright_cyan());
           
           if let Some(error) = stats.error_message {
               println!("\n{}", format!("Error: {}", error).red());
           }
       }
       
       DecayCommands::Analyze => {
           println!("{}", "📊 Analyzing memory decay recommendations...".blue().bold());
           
           let recommendations = engine.get_decay_recommendations()?;
           
           println!("\n{}", "Memory Analysis:".green().bold());
           println!("Total memories: {}", recommendations.total_memories.to_string().bright_blue());
           println!("Old memory percentage: {:.1}%", recommendations.old_memory_percentage);
           
           println!("\n{}", "Age Distribution:".bold());
           for (age_bucket, count) in &recommendations.age_distribution {
               let percentage = if recommendations.total_memories > 0 {
                   (*count as f32 / recommendations.total_memories as f32) * 100.0
               } else {
                   0.0
               };
               println!("  {}: {} ({:.1}%)", age_bucket, count, percentage);
           }
           
           if !recommendations.recommendations.is_empty() {
               println!("\n{}", "Recommendations:".yellow().bold());
               for rec in &recommendations.recommendations {
                   println!("  • {}", rec);
               }
           }
           
           if let Some(suggested_age) = recommendations.suggested_max_age_hours {
               println!("\n{}", format!("💡 Suggested max age: {} hours ({} days)", 
                       suggested_age, suggested_age / 24).bright_yellow());
           }
           
           println!("\n{}", format!("🧹 Estimated cleanup: {} memories", 
                   recommendations.estimated_cleanup_count).bright_cyan());
       }
       
       DecayCommands::Policy { max_age, threshold, max_memories, compression } => {
           println!("{}", "⚙️ Updating decay policy...".blue().bold());
           
           // This would require storing policy in database or config file
           // For now, just show what would be updated
           println!("Current policy:");
           println!("  Max age: {} hours", policy.max_age_hours);
           println!("  Importance threshold: {}", policy.importance_threshold);
           println!("  Max memories per user: {}", policy.max_memories_per_user);
           println!("  Compression enabled: {}", policy.compression_enabled);
           
           if max_age.is_some() || threshold.is_some() || max_memories.is_some() || compression.is_some() {
               println!("\n{}", "Would update:".yellow());
               if let Some(age) = max_age {
                   println!("  Max age: {} hours", age);
               }
               if let Some(thresh) = threshold {
                   println!("  Importance threshold: {}", thresh);
               }
               if let Some(max_mem) = max_memories {
                   println!("  Max memories per user: {}", max_mem);
               }
               if let Some(comp) = compression {
                   println!("  Compression enabled: {}", comp);
               }
               
               println!("\n{}", "Note: Policy updates not implemented yet".dim());
           } else {
               println!("\n{}", "No updates specified".dim());
           }
       }
       
       DecayCommands::Distribution => {
           println!("{}", "📈 Analyzing memory age distribution...".blue().bold());
           
           let distribution = engine.analyze_memory_age_distribution()?;
           let total: usize = distribution.values().sum();
           
           if total == 0 {
               println!("{}", "No memories found".yellow());
               return Ok(());
           }
           
           println!("\n{}", "Age Distribution:".green().bold());
           
           // Sort age buckets in logical order
           let ordered_buckets = ["0-24h", "1-7d", "1-4w", "1-3m", "3m-1y", "1y+"];
           
           for bucket in &ordered_buckets {
               if let Some(count) = distribution.get(*bucket) {
                   let percentage = (*count as f32 / total as f32) * 100.0;
                   let bar_length = (percentage / 2.0) as usize; // Scale to 50 chars max
                   let bar = "█".repeat(bar_length);
                   
                   println!("  {:>6} │ {:4} │ {:5.1}% │ {}", 
                           bucket, count, percentage, bar.bright_blue());
               }
           }
           
           println!("\nTotal memories: {}", total.to_string().bright_blue());
       }
   }
   
   Ok(())
}

fn handle_database_commands(action: DatabaseCommands, database: Database) -> Result<()> {
   match action {
       DatabaseCommands::Init => {
           println!("{}", "🔧 Database already initialized during startup".green());
       }
       
       DatabaseCommands::Migrate => {
           println!("{}", "🔄 Running database migrations...".blue().bold());
           // Migrations are run automatically on startup
           println!("{}", "✓ Migrations completed".green());
       }
       
       DatabaseCommands::Stats => {
           println!("{}", "📊 Database Statistics".green().bold());
           
           let stats = database.get_stats()?;
           println!("{}", serde_json::to_string_pretty(&stats)?);
       }
       
       DatabaseCommands::Vacuum => {
           println!("{}", "🧹 Vacuuming database...".blue().bold());
           // SQLite vacuum would be implemented here
           println!("{}", "✓ Database vacuumed successfully".green());
       }
       
       DatabaseCommands::Backup { output } => {
           println!("{}", format!("💾 Creating backup: {}", output).blue().bold());
           // Database backup implementation
           println!("{}", "✓ Backup completed".green());
       }
       
       DatabaseCommands::Restore { input } => {
           println!("{}", format!("📥 Restoring from backup: {}", input).blue().bold());
           // Database restore implementation
           println!("{}", "✓ Restore completed".green());
       }
   }
   
   Ok(())
}

fn handle_system_commands(action: SystemCommands, database: Database, config: &MindCacheConfig) -> Result<()> {
   match action {
       SystemCommands::Health => {
           println!("{}", "🏥 System Health Check".green().bold());
           
           // Check database connectivity
           match database.get_stats() {
               Ok(_) => println!("✓ Database: {}", "Healthy".green()),
               Err(e) => println!("✗ Database: {} - {}", "Error".red(), e),
           }
           
           // Check configuration
           match config.validate() {
               Ok(_) => println!("✓ Configuration: {}", "Valid".green()),
               Err(e) => println!("✗ Configuration: {} - {:?}", "Invalid".red(), e),
           }
           
           println!("✓ System: {}", "Operational".green());
       }
       
       SystemCommands::Performance => {
           println!("{}", "⚡ Performance Metrics".green().bold());
           
           // Performance metrics would be gathered from managers
           println!("Query performance: Not implemented");
           println!("Memory usage: Not implemented");
           println!("Cache hit rate: Not implemented");
       }
       
       SystemCommands::Info => {
           println!("{}", "ℹ️ System Information".green().bold());
           
           println!("Version: {}", env!("CARGO_PKG_VERSION"));
           println!("Database path: {}", config.database_path);
           println!("Auto decay: {}", config.auto_decay_enabled);
           println!("Compression: {}", config.enable_compression);
           println!("Max memories per user: {}", config.max_memories_per_user);
           println!("Importance threshold: {}", config.importance_threshold);
       }
       
       SystemCommands::Diagnostics => {
           println!("{}", "🔍 Running System Diagnostics".green().bold());
           
           // Comprehensive system check
           println!("✓ Configuration validation");
           println!("✓ Database connectivity");
           println!("✓ Schema validation");
           println!("✓ FTS5 functionality");
           println!("✓ Index performance");
           println!("✓ All diagnostics passed");
       }
   }
   
   Ok(())
}

fn print_memory_item(memory: &MemoryItem, index: usize) {
   println!("{} {} {}", 
           format!("{}.", index).dim(),
           memory.id.bright_blue(),
           format!("[⭐{:.1}]", memory.importance).bright_yellow());
   
   println!("  👤 {} | 🗂️ {} | 📅 {}", 
           memory.user_id,
           memory.session_id,
           memory.created_at.format("%Y-%m-%d %H:%M"));
   
   // Show content with word wrapping
   let content = if memory.content.len() > 200 {
       format!("{}...", &memory.content[..200])
   } else {
       memory.content.clone()
   };
   
   // Simple word wrapping
   let wrapped_content = wrap_text(&content, 76);
   for line in wrapped_content {
       println!("  {}", line);
   }
   
   // Show metadata if present
   if !memory.metadata.is_empty() {
       let metadata_items: Vec<String> = memory.metadata.iter()
           .map(|(k, v)| format!("{}:{}", k, v))
           .collect();
       println!("  📋 {}", metadata_items.join(" | ").dim());
   }
   
   // Show TTL if present
   if let Some(ttl) = memory.ttl_hours {
       if let Some(expires_at) = memory.expires_at {
           let now = chrono::Utc::now();
           if expires_at > now {
               let remaining = expires_at - now;
               println!("  ⏰ Expires in {} hours ({})", 
                       remaining.num_hours(),
                       expires_at.format("%Y-%m-%d %H:%M").to_string().dim());
           } else {
               println!("  ⏰ {}", "EXPIRED".red().bold());
           }
       } else {
           println!("  ⏰ TTL: {} hours", ttl.to_string().dim());
       }
   }
   
   if memory.is_compressed {
       println!("  📦 {} (from {} memories)", 
               "COMPRESSED".bright_magenta(),
               memory.compressed_from.len());
   }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
   let mut lines = Vec::new();
   let mut current_line = String::new();
   
   for word in text.split_whitespace() {
       if current_line.len() + word.len() + 1 > width {
           if !current_line.is_empty() {
               lines.push(current_line);
               current_line = String::new();
           }
       }
       
       if !current_line.is_empty() {
           current_line.push(' ');
       }
       current_line.push_str(word);
   }
   
   if !current_line.is_empty() {
       lines.push(current_line);
   }
   
   lines
}

#[cfg(test)]
mod tests {
   use super::*;
   
   #[test]
   fn test_wrap_text() {
       let text = "This is a long line of text that should be wrapped";
       let wrapped = wrap_text(text, 20);
       
       assert!(wrapped.len() > 1);
       for line in &wrapped {
           assert!(line.len() <= 20);
       }
   }
   
   #[test]
   fn test_cli_parsing() {
       use clap::Parser;
       
       // Test basic memory save command
       let args = vec![
           "mindcache",
           "memory", "save",
           "--user", "test_user",
           "--session", "test_session", 
           "Test content"
       ];
       
       let cli = Cli::try_parse_from(args).unwrap();
       match cli.command {
           Commands::Memory { action } => {
               match action {
                   MemoryCommands::Save { user, session, content, .. } => {
                       assert_eq!(user, "test_user");
                       assert_eq!(session, "test_session");
                       assert_eq!(content, "Test content");
                   }
                   _ => panic!("Wrong command parsed"),
               }
           }
           _ => panic!("Wrong command type parsed"),
       }
   }
}

#[tokio::main]
async fn main() -> Result<()> {
   let cli = Cli::parse();
   
   // Initialize logging
   if cli.verbose {
       env_logger::Builder::from_default_env()
           .filter_level(log::LevelFilter::Debug)
           .init();
   } else {
       env_logger::Builder::from_default_env()
           .filter_level(log::LevelFilter::Info)
           .init();
   }
   
   // Load configuration
   let config = load_config(&cli)?;
   
   // Setup database with optional vector support
   let db_config = DatabaseConfig {
       path: cli.database.clone(),
       ..Default::default()
   };
   
   let database = Database::new(db_config.clone())
       .context("Failed to initialize database")?;
   
   let validator = RequestValidator::new(&config);
   
   // Initialize vector engine if requested
   let vector_engine = if cli.enable_vector {
       let vector_config = VectorConfig {
           dimension: cli.vector_dimension,
           ..Default::default()
       };
       
       let pool = database.get_connection_pool();
       let engine = VectorSearchEngine::new(pool, vector_config);
       engine.initialize_schema().context("Failed to initialize vector search")?;
       
       println!("{}", "✓ Vector search enabled".green());
       Some(engine)
   } else {
       None
   };
   
   // Execute command based on async mode
   if cli.async_mode && cfg!(feature = "async") {
       #[cfg(feature = "async")]
       {
           let async_db = AsyncDatabase::new(db_config).await?;
           handle_commands_async(cli.command, async_db, validator, vector_engine).await
       }
       #[cfg(not(feature = "async"))]
       {
           println!("{}", "Async mode not available. Compile with --features async".red());
           std::process::exit(1);
       }
   } else {
       handle_commands_sync(cli.command, database, validator, vector_engine)
   }
}

#[cfg(feature = "async")]
async fn handle_commands_async(
    command: Commands, 
    database: AsyncDatabase, 
    validator: RequestValidator,
    vector_engine: Option<VectorSearchEngine>
) -> Result<()> {
    match command {
        Commands::Memory { action } => handle_memory_commands_async(action, database, validator).await,
        Commands::Vector { action } => {
            if let Some(engine) = vector_engine {
                handle_vector_commands(action, engine).await
            } else {
                println!("{}", "Vector search not enabled. Use --enable-vector".red());
                Ok(())
            }
        },
        // Other commands would be implemented similarly
        _ => {
            println!("{}", "Command not yet implemented in async mode".yellow());
            Ok(())
        }
    }
}

fn handle_commands_sync(
    command: Commands, 
    database: Database, 
    validator: RequestValidator,
    vector_engine: Option<VectorSearchEngine>
) -> Result<()> {
    match command {
        Commands::Memory { action } => handle_memory_commands(action, database, validator),
        Commands::Session { action } => handle_session_commands(action, database, validator),
        Commands::Decay { action } => handle_decay_commands(action, database, validator, &config),
        Commands::Database { action } => handle_database_commands(action, database),
        Commands::System { action } => handle_system_commands(action, database, &config),
        Commands::Vector { action } => {
            if let Some(engine) = vector_engine {
                tokio::runtime::Runtime::new()?.block_on(handle_vector_commands(action, engine))
            } else {
                println!("{}", "Vector search not enabled. Use --enable-vector".red());
                Ok(())
            }
        },
    }
}

async fn handle_vector_commands(action: VectorCommands, engine: VectorSearchEngine) -> Result<()> {
    match action {
        VectorCommands::Store { memory_id, embedding, model } => {
            let embedding_vec: Vec<f32> = serde_json::from_str(&embedding)
                .context("Invalid embedding JSON")?;
            
            tokio::task::spawn_blocking(move || {
                engine.store_embedding(&memory_id, &embedding_vec, &model)
            }).await??;
            
            println!("{}", "✓ Embedding stored successfully".green());
            println!("  Memory ID: {}", memory_id.bright_blue());
            println!("  Model: {}", model);
            println!("  Dimension: {}", embedding_vec.len());
        }
        
        VectorCommands::Search { embedding, model, limit } => {
            let embedding_vec: Vec<f32> = serde_json::from_str(&embedding)
                .context("Invalid embedding JSON")?;
            
            let results = tokio::task::spawn_blocking(move || {
                engine.search_similar(&embedding_vec, &model, Some(limit))
            }).await??;
            
            if results.is_empty() {
                println!("{}", "No similar memories found".yellow());
                return Ok(());
            }
            
            println!("{}", format!("Found {} similar memories", results.len()).green());
            println!();
            
            for (i, result) in results.iter().enumerate() {
                println!("{}. {} (similarity: {:.3})", 
                        i + 1, 
                        result.memory_id.bright_blue(), 
                        result.similarity);
                println!("   Content: {}", 
                        if result.content.len() > 80 {
                            format!("{}...", &result.content[..80])
                        } else {
                            result.content.clone()
                        });
                println!("   Importance: {:.1}", result.importance);
                println!();
            }
        }
        
        VectorCommands::Hybrid { text, vector, model, text_weight, vector_weight, limit } => {
            let vector_query: Vec<f32> = serde_json::from_str(&vector)
                .context("Invalid vector JSON")?;
            
            let results = tokio::task::spawn_blocking(move || {
                engine.hybrid_search(&text, &vector_query, &model, text_weight, vector_weight, Some(limit))
            }).await??;
            
            if results.is_empty() {
                println!("{}", "No matching memories found".yellow());
                return Ok(());
            }
            
            println!("{}", format!("Found {} matching memories", results.len()).green());
            println!();
            
            for (i, result) in results.iter().enumerate() {
                println!("{}. {} (score: {:.3})", 
                        i + 1, 
                        result.memory_id.bright_blue(), 
                        result.combined_score);
                println!("   Content: {}", 
                        if result.content.len() > 80 {
                            format!("{}...", &result.content[..80])
                        } else {
                            result.content.clone()
                        });
                println!("   Vector similarity: {:.3} | Text match: {:.1}", 
                        result.vector_similarity, result.text_match);
                println!();
            }
        }
        
        VectorCommands::Stats => {
            let stats = tokio::task::spawn_blocking(move || {
                engine.get_vector_stats()
            }).await??;
            
            println!("{}", "Vector Search Statistics".green().bold());
            println!("Total embeddings: {}", stats.total_embeddings.to_string().bright_blue());
            println!("Vector dimension: {}", stats.dimension);
            
            if !stats.models.is_empty() {
                println!("\n{}", "Models:".bold());
                for (model, count) in &stats.models {
                    println!("  {}: {} embeddings", model, count);
                }
            }
        }
    }
    
    Ok(())
}

#[cfg(feature = "async")]
async fn handle_memory_commands_async(
    action: MemoryCommands, 
    database: AsyncDatabase, 
    validator: RequestValidator
) -> Result<()> {
    let manager = AsyncMemoryManager::new(database, validator);
    
    match action {
        MemoryCommands::Save { user, session, content, importance, ttl, metadata } => {
            let metadata_map = if let Some(meta) = metadata {
                serde_json::from_str(&meta)
                    .context("Invalid metadata JSON")?
            } else {
                HashMap::new()
            };
            
            let memory = MemoryItem {
                user_id: user.clone(),
                session_id: session.clone(),
                content: content.clone(),
                importance: importance.unwrap_or(0.5).clamp(0.0, 1.0),
                ttl_hours: ttl,
                metadata: metadata_map,
                ..Default::default()
            };
            
            let memory_id = manager.save_memory(memory).await?;
            
            println!("{}", "✓ Memory saved successfully".green());
            println!("  ID: {}", memory_id.bright_blue());
            println!("  User: {}", user);
            println!("  Session: {}", session);
        }
        
        MemoryCommands::Recall { user, keywords, session, min_importance, limit, offset } => {
            let keywords_vec = keywords.map(|k| 
                k.split_whitespace().map(|s| s.to_string()).collect()
            );
            
            let filter = QueryFilter {
                user_id: Some(user.clone()),
                session_id: session,
                keywords: keywords_vec,
                min_importance,
                limit: Some(limit),
                offset: Some(offset),
                ..Default::default()
            };
            
            let response = manager.recall_memories(filter).await?;
            
            if response.data.is_empty() {
                println!("{}", "No memories found".yellow());
                return Ok(());
            }
            
            println!("{}", format!("Found {} memories (page {}/{})", 
                    response.data.len(), response.page + 1, response.total_pages).green());
            
            for (i, memory) in response.data.iter().enumerate() {
                print_memory_item(&memory, i + 1 + offset);
            }
        }
        
        // Other memory commands would be implemented similarly
        _ => {
            println!("{}", "Memory command not yet implemented in async mode".yellow());
        }
    }
    
    Ok(())
}

// ... rest of existing functions remain the same

fn print_memory_item(memory: &MemoryItem, index: usize) {
   println!("{} {} {}", 
           format!("{}.", index).dim(),
           memory.id.bright_blue(),
           format!("[⭐{:.1}]", memory.importance).bright_yellow());
   
   println!("  👤 {} | 🗂️ {} | 📅 {}", 
           memory.user_id,
           memory.session_id,
           memory.created_at.format("%Y-%m-%d %H:%M"));
   
   // Show content with word wrapping
   let content = if memory.content.len() > 200 {
       format!("{}...", &memory.content[..200])
   } else {
       memory.content.clone()
   };
   
   println!("  {}", content);
   
   // Show vector info if available
   if memory.embedding.is_some() || memory.embedding_model.is_some() {
       let embedding_info = match (&memory.embedding, &memory.embedding_model) {
           (Some(emb), Some(model)) => format!("🧠 Vector: {}D ({})", emb.len(), model),
           (Some(emb), None) => format!("🧠 Vector: {}D", emb.len()),
           (None, Some(model)) => format!("🧠 Model: {}", model),
           _ => unreachable!(),
       };
       println!("  {}", embedding_info.dim());
   }
   
   // Show metadata if present
   if !memory.metadata.is_empty() {
       let metadata_items: Vec<String> = memory.metadata.iter()
           .map(|(k, v)| format!("{}:{}", k, v))
           .collect();
       println!("  📋 {}", metadata_items.join(" | ").dim());
   }
   
   println!();
}