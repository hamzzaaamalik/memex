//! Foreign Function Interface (FFI) module for Node.js bindings

use std::collections::HashMap;
use std::sync::Mutex;

use crate::core::decay::DecayEngine;
use crate::core::memory::MemoryManager;
use crate::core::session::SessionManager;
use crate::core::{MindCacheConfig, RequestValidator};
use crate::database::models::*;
use crate::database::{Database, DatabaseConfig};

// Global state for FFI instances
static INSTANCE_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
static INSTANCES: once_cell::sync::Lazy<Mutex<HashMap<usize, MindCacheHandle>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

/// Basic MindCache FFI handle
#[allow(dead_code)]
pub struct MindCacheHandle {
    memory_manager: MemoryManager,
    session_manager: SessionManager,
    decay_engine: DecayEngine,
    config: MindCacheConfig,
}

// Main initialization function
pub fn create_mindcache_instance(
    config: MindCacheConfig,
) -> Result<usize, Box<dyn std::error::Error>> {
    init_with_config(config)
}

fn init_with_config(config: MindCacheConfig) -> Result<usize, Box<dyn std::error::Error>> {
    // Initialize database
    let db_config = DatabaseConfig {
        path: config.database_path.clone(),
        ..Default::default()
    };

    let database = Database::new(db_config)?;
    let validator = RequestValidator::new(&config);

    // Initialize core components
    let memory_manager = MemoryManager::new(database.clone(), validator.clone());
    let session_manager = SessionManager::new(database.clone(), validator.clone());
    let decay_policy = DecayPolicy::default();
    let decay_engine = DecayEngine::new(database, validator, decay_policy);

    // Create handle
    let handle = MindCacheHandle {
        memory_manager,
        session_manager,
        decay_engine,
        config,
    };

    // Generate unique instance ID
    let instance_id = INSTANCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    // Store instance
    let mut instances = INSTANCES.lock().unwrap();
    instances.insert(instance_id, handle);

    log::debug!("Created MindCache instance {}", instance_id);
    Ok(instance_id)
}

// Helper functions for internal use
pub fn get_instance(
    handle: usize,
) -> Option<std::sync::MutexGuard<'static, HashMap<usize, MindCacheHandle>>> {
    if handle == 0 {
        return None;
    }
    let instances = INSTANCES.lock().unwrap();
    if instances.contains_key(&handle) {
        Some(instances)
    } else {
        None
    }
}

// FFI functions are defined in lib.rs
