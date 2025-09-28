// examples/trading_journal.rs
//! Advanced Trading Journal Example
//! 
//! This example demonstrates using MindCache as a sophisticated trading journal
//! with features like:
//! - Trade logging with structured metadata
//! - Performance analysis
//! - Risk management tracking
//! - Market insight organization
//! - Batch operations for importing historical data

use mindcache_core::*;
use mindcache_core::database::models::*;
use std::collections::HashMap;
use chrono::{DateTime, Utc, Duration};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
struct Trade {
    symbol: String,
    action: String, // "buy" or "sell"
    quantity: u32,
    price: f64,
    timestamp: DateTime<Utc>,
    strategy: String,
    confidence: f32, // 0.0 to 1.0
    notes: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MarketInsight {
    title: String,
    content: String,
    category: String, // "technical", "fundamental", "macro", "sentiment"
    impact: String,   // "high", "medium", "low"
    timeframe: String, // "short", "medium", "long"
    confidence: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct RiskEvent {
    event_type: String, // "stop_loss", "position_size", "correlation", "exposure"
    description: String,
    severity: String, // "critical", "high", "medium", "low"
    action_taken: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("📈 MindCache Trading Journal Example");
    println!("====================================\n");

    // Setup MindCache for trading journal
    let config = MindCacheConfig {
        database_path: "./examples/trading_journal.db".to_string(),
        default_memory_ttl_hours: Some(8760), // 1 year
        enable_compression: true,
        max_memories_per_user: 50000, // Large capacity for extensive trading history
        importance_threshold: 0.2, // Keep even minor insights
        ..Default::default()
    };

    let db_config = DatabaseConfig {
        path: config.database_path.clone(),
        enable_wal: true,
        cache_size: -8000, // 8MB cache for better performance
        ..Default::default()
    };

    let database = Database::new(db_config)?;
    let validator = core::RequestValidator::new(&config);
    let memory_manager = core::memory::MemoryManager::new(database.clone(), validator.clone());
    let session_manager = core::session::SessionManager::new(database.clone(), validator.clone());

    let trader_id = "alice_trader";
    println!("✅ Initialized trading journal for trader: {}\n", trader_id);

    // Create sessions for different types of trading activities
    let day_trading_session = session_manager.create_session(
        trader_id, 
        Some("Day Trading - Tech Stocks".to_string())
    )?;
    
    let swing_trading_session = session_manager.create_session(
        trader_id, 
        Some("Swing Trading - Market Leaders".to_string())
    )?;
    
    let research_session = session_manager.create_session(
        trader_id, 
        Some("Market Research & Analysis".to_string())
    )?;
    
    println!("📁 Created trading sessions:");
    println!("   • Day Trading: {}", day_trading_session);
    println!("   • Swing Trading: {}", swing_trading_session);
    println!("   • Research: {}\n", research_session);

    // Simulate a day of trading activities
    println!("💼 Day 1: Active Trading Day");
    println!("----------------------------");

    // Morning: Pre-market analysis
    let market_insights = vec![
        MarketInsight {
            title: "SPY Testing Key Resistance".to_string(),
            content: "SPY approaching 450 resistance level with strong volume. \
                     Watch for breakout or rejection. VIX at 18 suggests moderate uncertainty.".to_string(),
            category: "technical".to_string(),
            impact: "high".to_string(),
            timeframe: "short".to_string(),
            confidence: 0.8,
        },
        MarketInsight {
            title: "Fed Speech at 2 PM EST".to_string(),
            content: "Jerome Powell speaking at 2 PM. Market expecting dovish tone \
                     on interest rates. Prepare for volatility in financial sector.".to_string(),
            category: "macro".to_string(),
            impact: "high".to_string(),
            timeframe: "short".to_string(),
            confidence: 0.9,
        },
        MarketInsight {
            title: "AAPL Earnings Next Week".to_string(),
            content: "Apple earnings on Tuesday after market close. Options activity \
                     suggests big move expected. iPhone sales in focus.".to_string(),
            category: "fundamental".to_string(),
            impact: "medium".to_string(),
            timeframe: "medium".to_string(),
            confidence: 0.7,
        },
    ];

    for insight in market_insights {
        let importance = match insight.impact.as_str() {
            "high" => 0.9,
            "medium" => 0.6,
            "low" => 0.3,
            _ => 0.5,
        };

        let mut metadata = HashMap::new();
        metadata.insert("category".to_string(), insight.category.clone());
        metadata.insert("impact".to_string(), insight.impact.clone());
        metadata.insert("timeframe".to_string(), insight.timeframe.clone());
        metadata.insert("confidence".to_string(), insight.confidence.to_string());

        let memory = MemoryItem {
            user_id: trader_id.to_string(),
            session_id: research_session.clone(),
            content: format!("{}: {}", insight.title, insight.content),
            importance,
            metadata,
            tags: vec!["analysis".to_string(), insight.category, "premarket".to_string()],
            ..Default::default()
        };

        memory_manager.save_memory(memory)?;
        println!("📊 Logged insight: {}", insight.title);
    }

    // Trading activity throughout the day
    let trades = vec![
        Trade {
            symbol: "TSLA".to_string(),
            action: "buy".to_string(),
            quantity: 50,
            price: 242.50,
            timestamp: Utc::now() - Duration::hours(6),
            strategy: "momentum_breakout".to_string(),
            confidence: 0.8,
            notes: "Clean breakout above 240 resistance with volume spike. \
                    RSI showing strength but not overbought.".to_string(),
        },
        Trade {
            symbol: "TSLA".to_string(),
            action: "sell".to_string(),
            quantity: 25,
            price: 248.75,
            timestamp: Utc::now() - Duration::hours(4),
            strategy: "partial_profit".to_string(),
            confidence: 0.7,
            notes: "Taking partial profits at 2.5% gain. Holding rest for potential \
                    continuation to 255 target.".to_string(),
        },
        Trade {
            symbol: "MSFT".to_string(),
            action: "buy".to_string(),
            quantity: 30,
            price: 378.25,
            timestamp: Utc::now() - Duration::hours(3),
            strategy: "support_bounce".to_string(),
            confidence: 0.6,
            notes: "Bounce off 375 support level. Fed speech risk but good R/R setup. \
                    Stop at 372.".to_string(),
        },
        Trade {
            symbol: "MSFT".to_string(),
            action: "sell".to_string(),
            quantity: 30,
            price: 374.80,
            timestamp: Utc::now() - Duration::hours(1),
            strategy: "stop_loss".to_string(),
            confidence: 0.9,
            notes: "Stopped out as Fed speech caused tech selloff. Good risk management. \
                    Small loss better than big loss.".to_string(),
        },
    ];

    for trade in trades {
        let pnl = match trade.action.as_str() {
            "sell" => {
                // This is simplified - in reality you'd track entry prices
                if trade.symbol == "TSLA" { 6.25 * trade.quantity as f64 } // Profit
                else { -3.45 * trade.quantity as f64 } // Loss
            },
            _ => 0.0,
        };

        let importance = if trade.action == "sell" {
            if pnl > 0.0 { 0.8 } else { 0.9 } // Losses are high importance for learning
        } else {
            0.6 // Entries are medium importance
        };

        let mut metadata = HashMap::new();
        metadata.insert("symbol".to_string(), trade.symbol.clone());
        metadata.insert("action".to_string(), trade.action.clone());
        metadata.insert("quantity".to_string(), trade.quantity.to_string());
        metadata.insert("price".to_string(), trade.price.to_string());
        metadata.insert("strategy".to_string(), trade.strategy.clone());
        metadata.insert("confidence".to_string(), trade.confidence.to_string());
        if pnl != 0.0 {
            metadata.insert("pnl".to_string(), format!("{:.2}", pnl));
        }

        let session = if trade.strategy.contains("momentum") || trade.strategy.contains("breakout") {
            &day_trading_session
        } else {
            &swing_trading_session
        };

        let content = format!(
            "{} {} shares of {} at ${:.2} using {} strategy. {}",
            trade.action.to_uppercase(),
            trade.quantity,
            trade.symbol,
            trade.price,
            trade.strategy,
            trade.notes
        );

        let memory = MemoryItem {
            user_id: trader_id.to_string(),
            session_id: session.clone(),
            content,
            importance,
            metadata,
            tags: vec!["trade".to_string(), trade.symbol.clone(), trade.action.clone()],
            created_at: trade.timestamp,
            updated_at: trade.timestamp,
            ..Default::default()
        };

        memory_manager.save_memory(memory)?;
        
        if pnl != 0.0 {
            println!("💰 Trade: {} {} shares at ${:.2} - P&L: ${:.2}", 
                    trade.action.to_uppercase(), trade.quantity, trade.price, pnl);
        } else {
            println!("📈 Trade: {} {} shares at ${:.2}", 
                    trade.action.to_uppercase(), trade.quantity, trade.price);
        }
    }

    // Risk management events
    let risk_events = vec![
        RiskEvent {
            event_type: "exposure_check".to_string(),
            description: "Portfolio 75% long tech before Fed speech - reducing to 50%".to_string(),
            severity: "medium".to_string(),
            action_taken: "Sold half MSFT position, tightened stops on others".to_string(),
        },
        RiskEvent {
            event_type: "correlation_risk".to_string(),
            description: "TSLA and MSFT moving in lockstep - diversification compromised".to_string(),
            severity: "low".to_string(),
            action_taken: "Note for future: avoid simultaneous tech positions during events".to_string(),
        },
    ];

    for risk_event in risk_events {
        let importance = match risk_event.severity.as_str() {
            "critical" => 1.0,
            "high" => 0.9,
            "medium" => 0.7,
            "low" => 0.5,
            _ => 0.5,
        };

        let mut metadata = HashMap::new();
        metadata.insert("event_type".to_string(), risk_event.event_type.clone());
        metadata.insert("severity".to_string(), risk_event.severity.clone());

        let content = format!(
            "RISK ALERT [{}]: {} - Action: {}",
            risk_event.severity.to_uppercase(),
            risk_event.description,
            risk_event.action_taken
        );

        let memory = MemoryItem {
            user_id: trader_id.to_string(),
            session_id: day_trading_session.clone(),
            content,
            importance,
            metadata,
            tags: vec!["risk".to_string(), risk_event.event_type.clone(), "management".to_string()],
            ..Default::default()
        };

        memory_manager.save_memory(memory)?;
        println!("⚠️  Risk event: {}", risk_event.description);
    }

    println!("\n📊 Day 1 Summary Complete\n");

    // Analysis and insights from the day
    println!("🔍 Analyzing Trading Performance");
    println!("--------------------------------");

    // Get all trades for analysis
    let all_trades = memory_manager.recall_memories(QueryFilter {
        user_id: Some(trader_id.to_string()),
        keywords: Some(vec!["trade".to_string()]),
        ..Default::default()
    })?;

    println!("📈 Total trades executed: {}", all_trades.total_count);

    // Get profitable vs losing trades
    let profitable_trades = memory_manager.recall_memories(QueryFilter {
        user_id: Some(trader_id.to_string()),
        keywords: Some(vec!["SELL".to_string(), "profit".to_string()]),
        ..Default::default()
    })?;

    let losing_trades = memory_manager.recall_memories(QueryFilter {
        user_id: Some(trader_id.to_string()),
        keywords: Some(vec!["stop".to_string(), "loss".to_string()]),
        ..Default::default()
    })?;

    println!("✅ Profitable trades: {}", profitable_trades.total_count);
    println!("❌ Losing trades: {}", losing_trades.total_count);

    // Get risk management events
    let risk_events_query = memory_manager.recall_memories(QueryFilter {
        user_id: Some(trader_id.to_string()),
        keywords: Some(vec!["risk".to_string()]),
        ..Default::default()
    })?;

    println!("⚠️  Risk events: {}", risk_events_query.total_count);

    // Get high-importance insights for review
    println!("\n⭐ High-Importance Insights for Review:");
    let important_insights = memory_manager.recall_memories(QueryFilter {
        user_id: Some(trader_id.to_string()),
        min_importance: Some(0.8),
        limit: Some(5),
        ..Default::default()
    })?;

    for insight in &important_insights.data {
        println!("  • {} (importance: {:.1})", 
                insight.content.chars().take(80).collect::<String>() + "...", 
                insight.importance);
    }

    // Demonstrate advanced querying
    println!("\n🔍 Advanced Querying Examples");
    println!("-----------------------------");

    // Find all TSLA-related activities
    let tsla_activities = memory_manager.recall_memories(QueryFilter {
        user_id: Some(trader_id.to_string()),
        keywords: Some(vec!["TSLA".to_string()]),
        ..Default::default()
    })?;

    println!("📊 TSLA-related activities: {}", tsla_activities.total_count);
    for activity in &tsla_activities.data {
        println!("  • {}", activity.content.chars().take(60).collect::<String>() + "...");
    }

    // Find momentum-based strategies
    let momentum_trades = memory_manager.recall_memories(QueryFilter {
        user_id: Some(trader_id.to_string()),
        keywords: Some(vec!["momentum".to_string(), "breakout".to_string()]),
        ..Default::default()
    })?;

    println!("\n🚀 Momentum strategy trades: {}", momentum_trades.total_count);

    // Session summaries
    println!("\n📋 Session Summaries");
    println!("-------------------");

    let sessions = session_manager.get_user_sessions(trader_id, Some(10), Some(0))?;
    for session in &sessions.data {
        println!("\n📁 Session: {}", session.name.as_ref().unwrap_or(&"Unnamed".to_string()));
        println!("   Memories: {}", session.memory_count);
        
        if session.memory_count > 0 {
            match session_manager.generate_session_summary(&session.id) {
                Ok(summary) => {
                    println!("   Summary: {}", summary.summary_text);
                    println!("   Key Topics: {:?}", summary.key_topics);
                    println!("   Avg Importance: {:.2}", summary.importance_score);
                }
                Err(_) => {
                    println!("   Summary: (Insufficient data)");
                }
            }
        }
    }

    // Batch import example: Historical performance data
    println