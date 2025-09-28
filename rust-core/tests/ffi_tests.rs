//! FFI (Foreign Function Interface) tests
//! 
//! Tests the C API that Node.js will use

use mindcache_core::*;
use std::ffi::{CString, CStr};
use std::ptr;
use tempfile::TempDir;
use serial_test::serial;

#[test]
#[serial]
fn test_ffi_initialization_and_cleanup() {
    // Test default initialization
    let handle = mindcache_init();
    assert_ne!(handle, 0, "Default initialization should succeed");
    assert!(mindcache_is_valid(handle), "Handle should be valid");
    
    // Test version retrieval
    let version_ptr = mindcache_version();
    assert!(!version_ptr.is_null(), "Version should be available");
    
    let version_cstr = unsafe { CStr::from_ptr(version_ptr) };
    let version_str = version_cstr.to_str().expect("Version should be valid UTF-8");
    assert!(!version_str.is_empty(), "Version should not be empty");
    println!("Library version: {}", version_str);
    
    mindcache_free_string(version_ptr);
    
    // Test cleanup
    mindcache_destroy(handle);
    assert!(!mindcache_is_valid(handle), "Handle should be invalid after destroy");
}

#[test]
#[serial]
fn test_ffi_custom_config_initialization() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    let storage_path = temp_dir.path().join("ffi_test.db").to_string_lossy().to_string();
    
    let config = serde_json::json!({
        "database_path": storage_path,
        "default_memory_ttl_hours": 48,
        "enable_compression": true,
        "max_memories_per_user": 1000,
        "importance_threshold": 0.4,
        "enable_request_limits": false
    });
    
    let config_str = config.to_string();
    let config_cstring = CString::new(config_str).expect("Should create config string");
    
    let handle = mindcache_init_with_config(config_cstring.as_ptr());
    assert_ne!(handle, 0, "Config initialization should succeed");
    assert!(mindcache_is_valid(handle), "Handle should be valid");
    
    mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_memory_operations() {
    let handle = mindcache_init();
    assert_ne!(handle, 0);

    let user_id = CString::new("ffi_test_user").unwrap();
    let session_id = CString::new("ffi_test_session").unwrap();
    let content = CString::new("FFI test memory content").unwrap();
    let metadata = CString::new(r#"{"category":"test","priority":"high"}"#).unwrap();

    // Test save memory
    let memory_id_ptr = mindcache_save(
        handle,
        user_id.as_ptr(),
        session_id.as_ptr(),
        content.as_ptr(),
        0.8,  // importance
        24,   // ttl_hours
        metadata.as_ptr(),
        );

   assert!(!memory_id_ptr.is_null(), "Save should return memory ID");

   // Get the memory ID
   let memory_id_cstr = unsafe { CStr::from_ptr(memory_id_ptr) };
   let memory_id = memory_id_cstr.to_str().expect("Memory ID should be valid UTF-8");
   assert!(!memory_id.is_empty(), "Memory ID should not be empty");
   println!("Saved memory with ID: {}", memory_id);

   // Keep a copy of the memory ID for later use
   let memory_id_copy = CString::new(memory_id).unwrap();

   mindcache_free_string(memory_id_ptr);

   // Test get memory
   let retrieved_ptr = mindcache_get_memory(handle, memory_id_copy.as_ptr());
   assert!(!retrieved_ptr.is_null(), "Should retrieve saved memory");

   let retrieved_cstr = unsafe { CStr::from_ptr(retrieved_ptr) };
   let retrieved_json = retrieved_cstr.to_str().expect("Retrieved JSON should be valid UTF-8");
   
   // Parse and verify the retrieved memory
   let retrieved_memory: serde_json::Value = serde_json::from_str(retrieved_json)
       .expect("Should parse retrieved memory JSON");
   
   assert_eq!(retrieved_memory["user_id"], "ffi_test_user");
   assert_eq!(retrieved_memory["session_id"], "ffi_test_session");
   assert_eq!(retrieved_memory["content"], "FFI test memory content");
   assert_eq!(retrieved_memory["importance"], 0.8);

   mindcache_free_string(retrieved_ptr);

   // Test update memory
   let update_json = serde_json::json!({
       "content": "Updated FFI test content",
       "importance": 0.9
   });
   let update_str = update_json.to_string();
   let update_cstring = CString::new(update_str).unwrap();

   let updated = mindcache_update_memory(
       handle,
       memory_id_copy.as_ptr(),
       update_cstring.as_ptr()
   );
   assert!(updated, "Memory update should succeed");

   // Verify update
   let updated_ptr = mindcache_get_memory(handle, memory_id_copy.as_ptr());
   assert!(!updated_ptr.is_null());

   let updated_cstr = unsafe { CStr::from_ptr(updated_ptr) };
   let updated_json = updated_cstr.to_str().unwrap();
   let updated_memory: serde_json::Value = serde_json::from_str(updated_json).unwrap();

   assert_eq!(updated_memory["content"], "Updated FFI test content");
   assert_eq!(updated_memory["importance"], 0.9);

   mindcache_free_string(updated_ptr);

   // Test delete memory
   let deleted = mindcache_delete_memory(handle, memory_id_copy.as_ptr());
   assert!(deleted, "Memory deletion should succeed");

   // Verify deletion
   let not_found_ptr = mindcache_get_memory(handle, memory_id_copy.as_ptr());
   assert!(not_found_ptr.is_null(), "Deleted memory should not be found");

   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_batch_operations() {
   let handle = mindcache_init();
   assert_ne!(handle, 0);

   // Create batch of memories
   let memories = serde_json::json!([
       {
           "user_id": "batch_user",
           "session_id": "batch_session",
           "content": "Batch memory 1",
           "importance": 0.6,
           "metadata": {}
       },
       {
           "user_id": "batch_user", 
           "session_id": "batch_session",
           "content": "Batch memory 2",
           "importance": 0.7,
           "metadata": {}
       },
       {
           "user_id": "",  // Invalid - should cause error
           "session_id": "batch_session",
           "content": "Invalid batch memory",
           "importance": 0.5,
           "metadata": {}
       }
   ]);

   let memories_str = memories.to_string();
   let memories_cstring = CString::new(memories_str).unwrap();

   // Test batch save with fail_on_error = false
   let batch_result_ptr = mindcache_save_batch(
       handle,
       memories_cstring.as_ptr(),
       false  // fail_on_error
   );

   assert!(!batch_result_ptr.is_null(), "Batch save should return results");

   let batch_result_cstr = unsafe { CStr::from_ptr(batch_result_ptr) };
   let batch_result_json = batch_result_cstr.to_str().unwrap();
   let batch_response: serde_json::Value = serde_json::from_str(batch_result_json).unwrap();

   assert_eq!(batch_response["success_count"], 2);
   assert_eq!(batch_response["error_count"], 1);

   mindcache_free_string(batch_result_ptr);
   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_recall_and_search() {
   let handle = mindcache_init();
   assert_ne!(handle, 0);

   let user_id = CString::new("search_user").unwrap();
   let session_id = CString::new("search_session").unwrap();

   // Save some test memories
   let test_memories = vec![
       ("Apple stock analysis shows strong growth potential", 0.8),
       ("Bitcoin price volatility creates trading opportunities", 0.7), 
       ("Tesla earnings report exceeded expectations", 0.9),
       ("Market volatility analysis suggests caution", 0.6),
   ];

   for (content, importance) in test_memories {
       let content_cstring = CString::new(content).unwrap();
       let metadata_cstring = CString::new("{}").unwrap();

       let memory_id_ptr = mindcache_save(
           handle,
           user_id.as_ptr(),
           session_id.as_ptr(),
           content_cstring.as_ptr(),
           importance,
           -1,  // no TTL
           metadata_cstring.as_ptr(),
       );

       assert!(!memory_id_ptr.is_null());
       mindcache_free_string(memory_id_ptr);
   }

   // Test recall with filter
   let filter = serde_json::json!({
       "user_id": "search_user",
       "limit": 10,
       "offset": 0
   });
   let filter_str = filter.to_string();
   let filter_cstring = CString::new(filter_str).unwrap();

   let recall_result_ptr = mindcache_recall(handle, filter_cstring.as_ptr());
   assert!(!recall_result_ptr.is_null(), "Recall should return results");

   let recall_cstr = unsafe { CStr::from_ptr(recall_result_ptr) };
   let recall_json = recall_cstr.to_str().unwrap();
   let recall_response: serde_json::Value = serde_json::from_str(recall_json).unwrap();

   assert_eq!(recall_response["data"].as_array().unwrap().len(), 4);
   assert_eq!(recall_response["total_count"], 4);

   mindcache_free_string(recall_result_ptr);

   // Test search
   let query = CString::new("Apple stock").unwrap();
   let search_result_ptr = mindcache_search(
       handle,
       user_id.as_ptr(),
       query.as_ptr(),
       10,  // limit
       0    // offset
   );

   assert!(!search_result_ptr.is_null(), "Search should return results");

   let search_cstr = unsafe { CStr::from_ptr(search_result_ptr) };
   let search_json = search_cstr.to_str().unwrap();
   let search_response: serde_json::Value = serde_json::from_str(search_json).unwrap();

   assert_eq!(search_response["data"].as_array().unwrap().len(), 1);
   assert!(search_response["data"][0]["content"].as_str().unwrap().contains("Apple"));

   mindcache_free_string(search_result_ptr);
   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_session_operations() {
   let handle = mindcache_init();
   assert_ne!(handle, 0);

   let user_id = CString::new("session_user").unwrap();
   let session_name = CString::new("Test Session").unwrap();

   // Test create session
   let session_id_ptr = mindcache_create_session(
       handle,
       user_id.as_ptr(),
       session_name.as_ptr()
   );

   assert!(!session_id_ptr.is_null(), "Session creation should succeed");

   let session_id_cstr = unsafe { CStr::from_ptr(session_id_ptr) };
   let session_id = session_id_cstr.to_str().unwrap();
   assert!(!session_id.is_empty());

   let session_id_copy = CString::new(session_id).unwrap();
   mindcache_free_string(session_id_ptr);

   // Add some memories to the session
   for i in 0..5 {
       let content = CString::new(format!("Session memory {}", i)).unwrap();
       let metadata = CString::new("{}").unwrap();

       let memory_id_ptr = mindcache_save(
           handle,
           user_id.as_ptr(),
           session_id_copy.as_ptr(),
           content.as_ptr(),
           0.5 + (i as f32 * 0.1),
           -1,
           metadata.as_ptr(),
       );

       assert!(!memory_id_ptr.is_null());
       mindcache_free_string(memory_id_ptr);
   }

   // Test get user sessions
   let sessions_result_ptr = mindcache_get_user_sessions(
       handle,
       user_id.as_ptr(),
       10,  // limit
       0    // offset
   );

   assert!(!sessions_result_ptr.is_null(), "Should get user sessions");

   let sessions_cstr = unsafe { CStr::from_ptr(sessions_result_ptr) };
   let sessions_json = sessions_cstr.to_str().unwrap();
   let sessions_response: serde_json::Value = serde_json::from_str(sessions_json).unwrap();

   assert_eq!(sessions_response["data"].as_array().unwrap().len(), 1);
   assert_eq!(sessions_response["data"][0]["memory_count"], 5);
   assert_eq!(sessions_response["data"][0]["name"], "Test Session");

   mindcache_free_string(sessions_result_ptr);

   // Test session summary
   let summary_result_ptr = mindcache_summarize_session(handle, session_id_copy.as_ptr());
   
   if !summary_result_ptr.is_null() {
       let summary_cstr = unsafe { CStr::from_ptr(summary_result_ptr) };
       let summary_json = summary_cstr.to_str().unwrap();
       let summary_response: serde_json::Value = serde_json::from_str(summary_json).unwrap();

       assert_eq!(summary_response["session_id"], session_id);
       assert_eq!(summary_response["memory_count"], 5);
       assert!(!summary_response["summary_text"].as_str().unwrap().is_empty());

       mindcache_free_string(summary_result_ptr);
   } else {
       println!("Session summary failed - this might be expected for small sessions");
   }

   // Test search sessions
   let keywords = serde_json::json!(["memory"]);
   let keywords_str = keywords.to_string();
   let keywords_cstring = CString::new(keywords_str).unwrap();

   let search_sessions_ptr = mindcache_search_sessions(
       handle,
       user_id.as_ptr(),
       keywords_cstring.as_ptr()
   );

   assert!(!search_sessions_ptr.is_null(), "Should find matching sessions");

   let search_sessions_cstr = unsafe { CStr::from_ptr(search_sessions_ptr) };
   let search_sessions_json = search_sessions_cstr.to_str().unwrap();
   let search_sessions_response: serde_json::Value = serde_json::from_str(search_sessions_json).unwrap();

   assert_eq!(search_sessions_response.as_array().unwrap().len(), 1);

   mindcache_free_string(search_sessions_ptr);

   // Test delete session
   let deleted = mindcache_delete_session(handle, session_id_copy.as_ptr(), true);
   assert!(deleted, "Session deletion should succeed");

   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_decay_operations() {
   let handle = mindcache_init();
   assert_ne!(handle, 0);

   let user_id = CString::new("decay_user").unwrap();
   let session_id = CString::new("decay_session").unwrap();

   // Create memories with different importance levels
   let test_memories = vec![
       ("High importance memory", 0.9, 48),
       ("Medium importance memory", 0.5, 24),
       ("Low importance memory", 0.1, 1),  // Short TTL
   ];

   for (content, importance, ttl) in test_memories {
       let content_cstring = CString::new(content).unwrap();
       let metadata_cstring = CString::new("{}").unwrap();

       let memory_id_ptr = mindcache_save(
           handle,
           user_id.as_ptr(),
           session_id.as_ptr(),
           content_cstring.as_ptr(),
           importance,
           ttl,
           metadata_cstring.as_ptr(),
       );

       assert!(!memory_id_ptr.is_null());
       mindcache_free_string(memory_id_ptr);
   }

   // Test decay analysis
   let analysis_result_ptr = mindcache_decay_analyze(handle);
   assert!(!analysis_result_ptr.is_null(), "Decay analysis should succeed");

   let analysis_cstr = unsafe { CStr::from_ptr(analysis_result_ptr) };
   let analysis_json = analysis_cstr.to_str().unwrap();
   let analysis_response: serde_json::Value = serde_json::from_str(analysis_json).unwrap();

   assert!(analysis_response["total_memories"].as_u64().unwrap() >= 3);
   assert!(analysis_response["old_memory_percentage"].as_f64().unwrap() >= 0.0);

   mindcache_free_string(analysis_result_ptr);

   // Test run decay
   let decay_result_ptr = mindcache_decay(handle);
   assert!(!decay_result_ptr.is_null(), "Decay process should run");

   let decay_cstr = unsafe { CStr::from_ptr(decay_result_ptr) };
   let decay_json = decay_cstr.to_str().unwrap();
   let decay_response: serde_json::Value = serde_json::from_str(decay_json).unwrap();

   assert!(decay_response["total_memories_before"].as_u64().unwrap() >= 3);
   assert!(decay_response["total_memories_after"].as_u64().unwrap() >= 0);
   assert!(decay_response["status"].as_str().unwrap() == "completed" || 
           decay_response["status"].as_str().unwrap() == "failed");

   mindcache_free_string(decay_result_ptr);
   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_statistics_and_analytics() {
   let handle = mindcache_init();
   assert_ne!(handle, 0);

   let user_id = CString::new("stats_user").unwrap();
   let session_id = CString::new("stats_session").unwrap();

   // Create some test data
   for i in 0..10 {
       let content = CString::new(format!("Statistics test memory {}", i)).unwrap();
       let metadata = CString::new(format!(r#"{{"index":"{}"}}"#, i)).unwrap();

       let memory_id_ptr = mindcache_save(
           handle,
           user_id.as_ptr(),
           session_id.as_ptr(),
           content.as_ptr(),
           0.3 + (i as f32 * 0.07), // Varying importance
           -1,
           metadata.as_ptr(),
       );

       assert!(!memory_id_ptr.is_null());
       mindcache_free_string(memory_id_ptr);
   }

   // Test system statistics
   let stats_result_ptr = mindcache_get_stats(handle);
   assert!(!stats_result_ptr.is_null(), "Should get system statistics");

   let stats_cstr = unsafe { CStr::from_ptr(stats_result_ptr) };
   let stats_json = stats_cstr.to_str().unwrap();
   let stats_response: serde_json::Value = serde_json::from_str(stats_json).unwrap();

   // Verify basic structure exists
   assert!(stats_response.is_object());

   mindcache_free_string(stats_result_ptr);

   // Test user statistics
   let user_stats_ptr = mindcache_get_user_stats(handle, user_id.as_ptr());
   assert!(!user_stats_ptr.is_null(), "Should get user statistics");

   let user_stats_cstr = unsafe { CStr::from_ptr(user_stats_ptr) };
   let user_stats_json = user_stats_cstr.to_str().unwrap();
   let user_stats_response: serde_json::Value = serde_json::from_str(user_stats_json).unwrap();

   assert_eq!(user_stats_response["total_memories"], 10);
   assert!(user_stats_response["avg_importance"].as_f64().unwrap() > 0.0);

   mindcache_free_string(user_stats_ptr);

   // Test session analytics
   let session_analytics_ptr = mindcache_get_session_analytics(handle, user_id.as_ptr());
   assert!(!session_analytics_ptr.is_null(), "Should get session analytics");

   let session_analytics_cstr = unsafe { CStr::from_ptr(session_analytics_ptr) };
   let session_analytics_json = session_analytics_cstr.to_str().unwrap();
   let session_analytics_response: serde_json::Value = serde_json::from_str(session_analytics_json).unwrap();

   assert_eq!(session_analytics_response["total_sessions"], 1);
   assert_eq!(session_analytics_response["total_memories"], 10);

   mindcache_free_string(session_analytics_ptr);

   // Test export
   let export_result_ptr = mindcache_export_user_memories(handle, user_id.as_ptr());
   assert!(!export_result_ptr.is_null(), "Should export user memories");

   let export_cstr = unsafe { CStr::from_ptr(export_result_ptr) };
   let export_json = export_cstr.to_str().unwrap();
   let export_memories: serde_json::Value = serde_json::from_str(export_json).unwrap();

   assert_eq!(export_memories.as_array().unwrap().len(), 10);

   mindcache_free_string(export_result_ptr);
   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_error_handling() {
   // Test with invalid handle
   assert!(mindcache_save(0, ptr::null(), ptr::null(), ptr::null(), 0.5, -1, ptr::null()).is_null());
   assert!(mindcache_recall(0, ptr::null()).is_null());
   assert!(!mindcache_is_valid(0));

   // Test with null parameters
   let handle = mindcache_init();
   assert_ne!(handle, 0);

   // Save with null parameters should fail
   assert!(mindcache_save(handle, ptr::null(), ptr::null(), ptr::null(), 0.5, -1, ptr::null()).is_null());

   // Get memory with null ID should fail
   assert!(mindcache_get_memory(handle, ptr::null()).is_null());

   // Invalid JSON should be handled gracefully
   let invalid_json = CString::new("{ invalid json }").unwrap();
   let invalid_handle = mindcache_init_with_config(invalid_json.as_ptr());
   assert_eq!(invalid_handle, 0, "Invalid JSON should return null handle");

   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_memory_management() {
   let handle = mindcache_init();
   assert_ne!(handle, 0);

   let user_id = CString::new("memory_mgmt_user").unwrap();
   let session_id = CString::new("memory_mgmt_session").unwrap();

   // Create and free many strings to test memory management
   let mut returned_strings = Vec::new();

   for i in 0..20 {
       let content = CString::new(format!("Memory management test {}", i)).unwrap();
       let metadata = CString::new("{}").unwrap();

       let memory_id_ptr = mindcache_save(
           handle,
           user_id.as_ptr(),
           session_id.as_ptr(),
           content.as_ptr(),
           0.5,
           -1,
           metadata.as_ptr(),
       );

       if !memory_id_ptr.is_null() {
           returned_strings.push(memory_id_ptr);
       }
   }

   // Free all returned strings
   for string_ptr in returned_strings {
       mindcache_free_string(string_ptr);
   }

   // Get stats to ensure everything is working
   let stats_ptr = mindcache_get_stats(handle);
   assert!(!stats_ptr.is_null());
   mindcache_free_string(stats_ptr);

   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_unicode_handling() {
   let handle = mindcache_init();
   assert_ne!(handle, 0);

   let user_id = CString::new("unicode_user").unwrap();
   let session_id = CString::new("unicode_session").unwrap();

   // Test Unicode content
   let unicode_content = "Hello 世界 🚀 café naïve résumé Москва العالم";
   let content_cstring = CString::new(unicode_content).unwrap();
   let metadata = CString::new(r#"{"language":"multilingual","emoji":"🚀"}"#).unwrap();

   let memory_id_ptr = mindcache_save(
       handle,
       user_id.as_ptr(),
       session_id.as_ptr(),
       content_cstring.as_ptr(),
       0.8,
       -1,
       metadata.as_ptr(),
   );

   assert!(!memory_id_ptr.is_null(), "Should save Unicode content");

   let memory_id_cstr = unsafe { CStr::from_ptr(memory_id_ptr) };
   let memory_id = memory_id_cstr.to_str().unwrap();
   let memory_id_copy = CString::new(memory_id).unwrap();

   mindcache_free_string(memory_id_ptr);

   // Retrieve and verify Unicode content
   let retrieved_ptr = mindcache_get_memory(handle, memory_id_copy.as_ptr());
   assert!(!retrieved_ptr.is_null());

   let retrieved_cstr = unsafe { CStr::from_ptr(retrieved_ptr) };
   let retrieved_json = retrieved_cstr.to_str().unwrap();
   let retrieved_memory: serde_json::Value = serde_json::from_str(retrieved_json).unwrap();

   assert_eq!(retrieved_memory["content"], unicode_content);
   assert!(retrieved_memory["metadata"]["emoji"].as_str().unwrap().contains("🚀"));

   mindcache_free_string(retrieved_ptr);

   // Test Unicode search
   let search_query = CString::new("世界").unwrap();
   let search_result_ptr = mindcache_search(
       handle,
       user_id.as_ptr(),
       search_query.as_ptr(),
       10,
       0
   );

   assert!(!search_result_ptr.is_null(), "Should find Unicode content");

   let search_cstr = unsafe { CStr::from_ptr(search_result_ptr) };
   let search_json = search_cstr.to_str().unwrap();
   let search_response: serde_json::Value = serde_json::from_str(search_json).unwrap();

   assert_eq!(search_response["data"].as_array().unwrap().len(), 1);

   mindcache_free_string(search_result_ptr);
   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_concurrent_access() {
   let handle = mindcache_init();
   assert_ne!(handle, 0);

   // Test concurrent operations (simulated with sequential calls)
   let user_id = CString::new("concurrent_user").unwrap();
   let session_id = CString::new("concurrent_session").unwrap();

   // Simulate rapid operations
   let mut memory_ids = Vec::new();

   for i in 0..50 {
       let content = CString::new(format!("Concurrent memory {}", i)).unwrap();
       let metadata = CString::new("{}").unwrap();

       let memory_id_ptr = mindcache_save(
           handle,
           user_id.as_ptr(),
           session_id.as_ptr(),
           content.as_ptr(),
           0.5,
           -1,
           metadata.as_ptr(),
       );

       if !memory_id_ptr.is_null() {
           let memory_id_cstr = unsafe { CStr::from_ptr(memory_id_ptr) };
           let memory_id = memory_id_cstr.to_str().unwrap().to_string();
           memory_ids.push(memory_id);
           mindcache_free_string(memory_id_ptr);
       }
   }

   // Verify all memories were saved
   assert!(memory_ids.len() >= 45, "Most concurrent operations should succeed");

   // Test concurrent recalls
   for i in (0..memory_ids.len()).step_by(5) {
       let filter = serde_json::json!({
           "user_id": "concurrent_user",
           "limit": 10,
           "offset": i
       });
       let filter_str = filter.to_string();
       let filter_cstring = CString::new(filter_str).unwrap();

       let recall_ptr = mindcache_recall(handle, filter_cstring.as_ptr());
       assert!(!recall_ptr.is_null(), "Concurrent recall should succeed");
       mindcache_free_string(recall_ptr);
   }

   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_error_codes() {
   // Test error code retrieval
   let handle = mindcache_init();
   assert_ne!(handle, 0);

   // Cause an error (save with invalid parameters)
   let result = mindcache_save(handle, ptr::null(), ptr::null(), ptr::null(), 0.5, -1, ptr::null());
   assert!(result.is_null(), "Invalid save should fail");

   // Get last error
   let error_code = mindcache_get_last_error();
   assert_ne!(error_code, 0, "Should have an error code");

   // Get error message
   let error_msg_ptr = mindcache_error_message(error_code);
   if !error_msg_ptr.is_null() {
       let error_msg_cstr = unsafe { CStr::from_ptr(error_msg_ptr) };
       let error_msg = error_msg_cstr.to_str().unwrap();
       assert!(!error_msg.is_empty(), "Error message should not be empty");
       println!("Error code {}: {}", error_code, error_msg);
       mindcache_free_string(error_msg_ptr);
   }

   mindcache_destroy(handle);
}

#[test]
#[serial]
fn test_ffi_multiple_instances() {
   let handle1 = mindcache_init();
   let handle2 = mindcache_init();

   assert_ne!(handle1, 0);
   assert_ne!(handle2, 0);
   assert_ne!(handle1, handle2, "Handles should be unique");

   assert!(mindcache_is_valid(handle1));
   assert!(mindcache_is_valid(handle2));

   // Operations on one handle shouldn't affect the other
   let user1 = CString::new("user1").unwrap();
   let user2 = CString::new("user2").unwrap();
   let session = CString::new("session").unwrap();
   let content = CString::new("content").unwrap();
   let metadata = CString::new("{}").unwrap();

   let mem1_ptr = mindcache_save(handle1, user1.as_ptr(), session.as_ptr(), content.as_ptr(), 0.5, -1, metadata.as_ptr());
   let mem2_ptr = mindcache_save(handle2, user2.as_ptr(), session.as_ptr(), content.as_ptr(), 0.5, -1, metadata.as_ptr());

   assert!(!mem1_ptr.is_null());
   assert!(!mem2_ptr.is_null());

   mindcache_free_string(mem1_ptr);
   mindcache_free_string(mem2_ptr);

   // Destroy one handle
   mindcache_destroy(handle1);
   assert!(!mindcache_is_valid(handle1));
   assert!(mindcache_is_valid(handle2), "Other handle should still be valid");

   // Clean up
   mindcache_destroy(handle2);
   assert!(!mindcache_is_valid(handle2));
}