//! MindCache Core Library
//!
//! A high-performance memory management system with intelligent decay,
//! full-text search, and session organization capabilities.

pub mod database;
pub mod core;
pub mod ffi;
pub mod cli;

#[cfg(feature = "async")]
pub mod async_db {
    pub use crate::database::async_db::*;
}

// Re-export commonly used types
pub use database::{Database, DatabaseConfig};
pub use database::simple_db::SimpleDatabase;
pub use database::models::*;
pub use core::*;

// Re-export async types when feature is enabled
#[cfg(feature = "async")]
pub use database::async_db::AsyncDatabase;

// Re-export vector types when feature is enabled
#[cfg(feature = "vector-search")]
pub use database::vector::{VectorConfig, VectorSearchEngine, VectorSearchResult, HybridSearchResult};

// FFI implementations using actual database
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::collections::HashMap;
use std::sync::Mutex;

// Global instance storage for FFI
static INSTANCE_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
static INSTANCES: once_cell::sync::Lazy<Mutex<HashMap<usize, SimpleDatabase>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

#[no_mangle]
pub extern "C" fn mindcache_init() -> usize {
    mindcache_init_with_config(ptr::null())
}

#[no_mangle]
pub extern "C" fn mindcache_init_with_config(config_json: *const c_char) -> usize {
    let result = std::panic::catch_unwind(|| -> Option<usize> {
        // Parse config if provided, otherwise use default
        let config = if config_json.is_null() {
            DatabaseConfig::default()
        } else {
            let config_str = unsafe {
                match CStr::from_ptr(config_json).to_str() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error converting config string: {}", e);
                        return None;
                    }
                }
            };

            let parsed_config: MindCacheConfig = match serde_json::from_str(config_str) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Error parsing config JSON: {}", e);
                    return None;
                }
            };

            DatabaseConfig {
                path: parsed_config.database_path,
                ..Default::default()
            }
        };

        // Create database instance using SimpleDatabase
        let database = match SimpleDatabase::new(config) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Error creating database: {}", e);
                return None;
            }
        };

        // Generate unique instance ID
        let instance_id = INSTANCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Store instance
        match INSTANCES.lock() {
            Ok(mut instances) => {
                instances.insert(instance_id, database);
                Some(instance_id)
            },
            Err(e) => {
                eprintln!("Error storing instance: {}", e);
                None
            }
        }
    });

    match result {
        Ok(Some(id)) => id,
        Ok(None) => 0,
        Err(e) => {
            eprintln!("Panic in mindcache_init_with_config: {:?}", e);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn mindcache_is_valid(handle: usize) -> bool {
    if handle == 0 {
        return false;
    }
    let instances = INSTANCES.lock().unwrap();
    instances.contains_key(&handle)
}

#[no_mangle]
pub extern "C" fn mindcache_destroy(handle: usize) {
    if handle == 0 {
        return;
    }
    let mut instances = INSTANCES.lock().unwrap();
    instances.remove(&handle);
}

#[no_mangle]
pub extern "C" fn mindcache_save(
    handle: usize,
    user_id: *const c_char,
    session_id: *const c_char,
    content: *const c_char,
    importance: f32,
    ttl_hours: i32,
    metadata_json: *const c_char,
) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        if handle == 0 {
            return None;
        }

        // Get database instance
        let instances = INSTANCES.lock().unwrap();
        let database = instances.get(&handle)?;

        // Parse parameters
        let user_id_str = if user_id.is_null() {
            return None;
        } else {
            unsafe { CStr::from_ptr(user_id).to_str().ok()? }
        };

        let session_id_str = if session_id.is_null() {
            return None;
        } else {
            unsafe { CStr::from_ptr(session_id).to_str().ok()? }
        };

        let content_str = if content.is_null() {
            return None;
        } else {
            unsafe { CStr::from_ptr(content).to_str().ok()? }
        };

        let metadata = if metadata_json.is_null() {
            std::collections::HashMap::new()
        } else {
            let metadata_str = unsafe { CStr::from_ptr(metadata_json).to_str().ok()? };
            serde_json::from_str(metadata_str).unwrap_or_default()
        };

        // Create memory item
        let memory = MemoryItem {
            id: String::new(), // Will be generated by database
            user_id: user_id_str.to_string(),
            session_id: session_id_str.to_string(),
            content: content_str.to_string(),
            content_vector: None,
            #[cfg(feature = "vector-search")]
            embedding: None,
            #[cfg(feature = "vector-search")]
            embedding_model: None,
            metadata,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: if ttl_hours > 0 {
                Some(chrono::Utc::now() + chrono::Duration::hours(ttl_hours as i64))
            } else {
                None
            },
            importance: importance.clamp(0.0, 1.0),
            ttl_hours: if ttl_hours > 0 { Some(ttl_hours as u32) } else { None },
            is_compressed: false,
            compressed_from: Vec::new(),
        };

        // Save to database
        let memory_id = database.save_memory(&memory).ok()?;
        Some(memory_id)
    });

    match result.unwrap_or(None) {
        Some(id) => match CString::new(id) {
            Ok(cstring) => cstring.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        None => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn mindcache_get_memory(handle: usize, memory_id: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        if handle == 0 || memory_id.is_null() {
            return None;
        }

        let instances = INSTANCES.lock().unwrap();
        let database = instances.get(&handle)?;

        let memory_id_str = unsafe { CStr::from_ptr(memory_id).to_str().ok()? };
        let memory = database.get_memory(memory_id_str).ok()??;

        let json = serde_json::to_string(&memory).ok()?;
        Some(json)
    });

    match result.unwrap_or(None) {
        Some(json) => match CString::new(json) {
            Ok(cstring) => cstring.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        None => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn mindcache_recall(handle: usize, filter_json: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        if handle == 0 {
            return None;
        }

        let instances = INSTANCES.lock().unwrap();
        let database = instances.get(&handle)?;

        let filter = if filter_json.is_null() {
            QueryFilter::default()
        } else {
            let filter_str = unsafe { CStr::from_ptr(filter_json).to_str().ok()? };
            serde_json::from_str(filter_str).unwrap_or_default()
        };

        let response = database.recall_memories(&filter).ok()?;
        let json = serde_json::to_string(&response).ok()?;
        Some(json)
    });

    match result.unwrap_or(None) {
        Some(json) => match CString::new(json) {
            Ok(cstring) => cstring.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        None => {
            // Return empty response structure instead of null
            let empty_response = serde_json::json!({
                "data": [],
                "total_count": 0,
                "page": 0,
                "per_page": 50,
                "total_pages": 0,
                "has_next": false,
                "has_prev": false
            });
            match CString::new(empty_response.to_string()) {
                Ok(cstring) => cstring.into_raw(),
                Err(_) => ptr::null_mut(),
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn mindcache_get_last_error() -> i32 {
    0 // No error for now - could implement error tracking later
}

#[no_mangle]
pub extern "C" fn mindcache_error_message(error_code: i32) -> *mut c_char {
    let message = match error_code {
        0 => "Success",
        _ => "Unknown error",
    };

    match CString::new(message) {
        Ok(cstring) => cstring.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn mindcache_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

#[no_mangle]
pub extern "C" fn mindcache_version() -> *mut c_char {
    match CString::new(env!("CARGO_PKG_VERSION")) {
        Ok(cstring) => cstring.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

// Stub implementations for functions not yet implemented
#[no_mangle] pub extern "C" fn mindcache_save_batch(_h: usize, _m: *const c_char, _f: bool) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_search(_h: usize, _u: *const c_char, _q: *const c_char, _l: i32, _o: i32) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_update_memory(_h: usize, _m: *const c_char, _u: *const c_char) -> bool { false }
#[no_mangle] pub extern "C" fn mindcache_delete_memory(_h: usize, _m: *const c_char) -> bool { false }
#[no_mangle] pub extern "C" fn mindcache_create_session(_h: usize, _u: *const c_char, _n: *const c_char) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_get_user_sessions(_h: usize, _u: *const c_char, _l: i32, _o: i32) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_summarize_session(_h: usize, _s: *const c_char) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_search_sessions(_h: usize, _u: *const c_char, _k: *const c_char) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_delete_session(_h: usize, _s: *const c_char, _d: bool) -> bool { false }
#[no_mangle] pub extern "C" fn mindcache_decay(_h: usize) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_decay_analyze(_h: usize) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_update_decay_policy(_h: usize, _p: *const c_char) -> bool { false }
#[no_mangle] pub extern "C" fn mindcache_get_stats(_h: usize) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_export_user_memories(_h: usize, _u: *const c_char) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_get_user_stats(_h: usize, _u: *const c_char) -> *mut c_char { ptr::null_mut() }
#[no_mangle] pub extern "C" fn mindcache_get_session_analytics(_h: usize, _u: *const c_char) -> *mut c_char { ptr::null_mut() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        let config = MindCacheConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_memory_item_creation() {
        let memory = MemoryItem::default();
        assert_eq!(memory.importance, 0.5);
    }

    #[test]
    fn test_ffi_basic() {
        let handle = mindcache_init();
        assert_ne!(handle, 0);
        assert!(mindcache_is_valid(handle));
        mindcache_destroy(handle);
    }
}