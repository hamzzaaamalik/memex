const ffi = require('ffi-napi');
const ref = require('ref-napi');
const path = require('path');
const fs = require('fs');

/**
 * Rust Bridge - FFI interface to Memex Rust core
 *
 * This module provides a JavaScript interface to the Rust storage engine
 * using Node.js FFI (Foreign Function Interface)
 */

class RustBridge {
  constructor(config = {}) {
    this.config = {
      database_path: config.storage_path || config.database_path || './memex.db',
      default_memory_ttl_hours: config.default_memory_ttl_hours || 720, // 30 days
      auto_decay_enabled: config.auto_decay_enabled !== false,
      decay_interval_hours: config.decay_interval_hours || 24,
      enable_compression: config.enable_compression !== false,
      max_memories_per_user: config.max_memories_per_user || 10000,
      importance_threshold: config.importance_threshold || 0.3,
      enable_request_limits: config.enable_request_limits !== false,
      max_requests_per_minute: config.max_requests_per_minute || 1000,
      max_batch_size: config.max_batch_size || 100
    };

    this.rustLib = null;
    this.handle = 0; // Changed from cachePtr to handle (integer)
    this.isInitialized = false;
  }

  /**
   * Initialize the Rust bridge and load the native library
   */
  async initialize() {
    try {
      console.log('🔗 Initializing Rust bridge...');

      // Load the Rust library
      await this.loadRustLibrary();

      // Initialize Memex with configuration
      await this.initializeMemex();

      this.isInitialized = true;
      console.log('✅ Rust bridge initialized successfully');
    } catch (error) {
      console.error('❌ Failed to initialize Rust bridge:', error);
      throw new Error(`Rust bridge initialization failed: ${error.message}`);
    }
  }

  /**
   * Load the Rust native library using FFI
   */
  async loadRustLibrary() {
    // Determine library path based on platform
    const libPath = this.getLibraryPath();

    console.log(`📚 Loading Rust library from: ${libPath}`);

    // Verify library exists
    if (!fs.existsSync(libPath)) {
      throw new Error(`Rust library not found at: ${libPath}. Please build the Rust core first.`);
    }

    // Define FFI interface - Updated to match new Rust API
    this.rustLib = ffi.Library(libPath, {
      // Core initialization
      memex_init: ['size_t', []],
      memex_init_with_config: ['size_t', ['string']],
      memex_destroy: ['void', ['size_t']],
      memex_is_valid: ['bool', ['size_t']],

      // Memory operations - Updated signatures
      memex_save: ['string', ['size_t', 'string', 'string', 'string', 'float', 'int', 'string']],
      memex_save_batch: ['string', ['size_t', 'string', 'bool']],
      memex_recall: ['string', ['size_t', 'string']],
      memex_search: ['string', ['size_t', 'string', 'string', 'int', 'int']],
      memex_get_memory: ['string', ['size_t', 'string']],
      memex_update_memory: ['bool', ['size_t', 'string', 'string']],
      memex_delete_memory: ['bool', ['size_t', 'string']],

      // Session operations
      memex_create_session: ['string', ['size_t', 'string', 'string']],
      memex_get_user_sessions: ['string', ['size_t', 'string', 'int', 'int']],
      memex_summarize_session: ['string', ['size_t', 'string']],
      memex_search_sessions: ['string', ['size_t', 'string', 'string']],
      memex_delete_session: ['bool', ['size_t', 'string', 'bool']],

      // Decay operations
      memex_decay: ['string', ['size_t']],
      memex_decay_analyze: ['string', ['size_t']],
      memex_update_decay_policy: ['bool', ['size_t', 'string']],

      // Statistics and utilities
      memex_get_stats: ['string', ['size_t']],
      memex_export_user_memories: ['string', ['size_t', 'string']],
      memex_get_user_stats: ['string', ['size_t', 'string']],
      memex_get_session_analytics: ['string', ['size_t', 'string']],

      // Error handling
      memex_get_last_error: ['int', []],
      memex_error_message: ['string', ['int']],

      // Utility functions
      memex_free_string: ['void', ['string']],
      memex_version: ['string', []]
    });

    console.log('✅ Rust library loaded successfully');
  }

  /**
   * Get the correct library path for the current platform
   */
  getLibraryPath() {
    const platform = process.platform;

    // Updated to match new project structure
    const baseDir = path.join(__dirname, '../rust-core/target');

    let libName;

    switch (platform) {
      case 'win32':
        libName = 'memex_core.dll';
        break;
      case 'darwin':
        libName = 'libmemex_core.dylib';
        break;
      case 'linux':
        libName = 'libmemex_core.so';
        break;
      default:
        throw new Error(`Unsupported platform: ${platform}`);
    }

    // Try different build configurations
    const possiblePaths = [
      path.join(baseDir, 'release', libName),
      path.join(baseDir, 'debug', libName),
      path.join(__dirname, libName), // Current directory fallback
      path.join(__dirname, '..', 'rust-core', 'target', 'release', libName),
      path.join(__dirname, '..', 'rust-core', 'target', 'debug', libName)
    ];

    console.log('🔍 Searching for Rust library...');
    for (const libPath of possiblePaths) {
      console.log(`   Checking: ${libPath}`);
      if (fs.existsSync(libPath)) {
        console.log(`✅ Found Rust library: ${libPath}`);
        return libPath;
      }
    }

    // List what files actually exist in target directories
    const releaseDir = path.join(baseDir, 'release');
    const debugDir = path.join(baseDir, 'debug');

    console.log('\n📁 Available files:');
    if (fs.existsSync(releaseDir)) {
      const releaseFiles = fs.readdirSync(releaseDir);
      console.log(`   Release dir: ${releaseFiles.join(', ')}`);
    } else {
      console.log(`   Release dir does not exist: ${releaseDir}`);
    }

    if (fs.existsSync(debugDir)) {
      const debugFiles = fs.readdirSync(debugDir);
      console.log(`   Debug dir: ${debugFiles.join(', ')}`);
    } else {
      console.log(`   Debug dir does not exist: ${debugDir}`);
    }

    throw new Error(`Rust library not found. Tried: ${possiblePaths.join(', ')}\n\nPlease build the Rust core first:\n  cd rust-core\n  cargo build --release`);
  }

  /**
   * Initialize Memex with configuration
   */
  async initializeMemex() {
    // Convert Node.js config to Rust config format
    const rustConfig = {
      database_path: this.config.database_path || this.config.storage_path || './memex.db',
      default_memory_ttl_hours: this.config.default_memory_ttl_hours,
      auto_decay_enabled: this.config.auto_decay_enabled,
      decay_interval_hours: this.config.decay_interval_hours,
      enable_compression: this.config.enable_compression,
      max_memories_per_user: this.config.max_memories_per_user,
      importance_threshold: this.config.importance_threshold,
      enable_request_limits: this.config.enable_request_limits,
      max_requests_per_minute: this.config.max_requests_per_minute,
      max_batch_size: this.config.max_batch_size
    };

    const configJson = JSON.stringify(rustConfig);
    console.log('⚙️ Initializing Memex with config:', configJson);

    // Initialize with configuration
    this.handle = this.rustLib.memex_init_with_config(configJson);

    if (this.handle === 0) {
      // Fallback to default initialization
      console.warn('⚠️ Config initialization failed, trying default...');
      this.handle = this.rustLib.memex_init();

      if (this.handle === 0) {
        const errorCode = this.rustLib.memex_get_last_error();
        const errorMessage = this.rustLib.memex_error_message(errorCode);
        throw new Error(`Failed to initialize Memex: ${errorMessage || 'Unknown error'}`);
      }
    }

    // Verify handle is valid
    if (!this.rustLib.memex_is_valid(this.handle)) {
      throw new Error('Invalid Memex handle after initialization');
    }

    console.log(`✅ Memex core initialized (handle: ${this.handle})`);
  }

  /**
   * Save a memory item - Updated to match new API
   */
  async saveMemory({ userId, sessionId, content, metadata = {}, importance = 0.5, ttlHours = null }) {
    this.ensureInitialized();

    try {
      const metadataJson = JSON.stringify(metadata);
      const ttl = ttlHours || -1; // -1 means no TTL

      console.log(`💾 Saving memory for user ${userId}, session ${sessionId}`);

      const result = this.rustLib.memex_save(
        this.handle,
        userId,
        sessionId,
        content,
        importance,
        ttl,
        metadataJson
      );

      if (!result) {
        const errorCode = this.rustLib.memex_get_last_error();
        const errorMessage = this.rustLib.memex_error_message(errorCode);
        throw new Error(`Failed to save memory: ${errorMessage || 'Unknown error'}`);
      }

      console.log(`✅ Memory saved with ID: ${result}`);
      return result;
    } catch (error) {
      console.error('❌ Error saving memory:', error);
      throw new Error(`Failed to save memory: ${error.message}`);
    }
  }

  /**
   * Save multiple memories in batch
   */
  async saveMemoriesBatch(memories, failOnError = false) {
    this.ensureInitialized();

    try {
      const memoriesJson = JSON.stringify(memories);

      console.log(`💾 Batch saving ${memories.length} memories`);

      const result = this.rustLib.memex_save_batch(
        this.handle,
        memoriesJson,
        failOnError
      );

      if (!result) {
        throw new Error('Failed to save memory batch - no result returned');
      }

      const batchResponse = JSON.parse(result);
      console.log(`✅ Batch save completed: ${batchResponse.success_count}/${batchResponse.results.length} successful`);
      
      return batchResponse;
    } catch (error) {
      console.error('❌ Error in batch save:', error);
      throw new Error(`Failed to save memory batch: ${error.message}`);
    }
  }

  /**
   * Recall memories with filters - Updated to use new filter format
   */
  async recallMemories(filter) {
    this.ensureInitialized();

    try {
      // Convert to new filter format
      const queryFilter = {
        user_id: filter.userId || null,
        session_id: filter.sessionId || null,
        keywords: filter.query ? filter.query.split(' ') : null,
        date_from: filter.dateFrom || null,
        date_to: filter.dateTo || null,
        limit: filter.limit || 50,
        offset: filter.offset || 0,
        min_importance: filter.minImportance || null
      };

      const filterJson = JSON.stringify(queryFilter);

      console.log(`🔍 Recalling memories with filter:`, queryFilter);

      const result = this.rustLib.memex_recall(this.handle, filterJson);

      if (!result) {
        return { data: [], total_count: 0, page: 0, per_page: 50, total_pages: 0, has_next: false, has_prev: false };
      }

      const response = JSON.parse(result);
      console.log(`✅ Recalled ${response.data.length} memories (${response.total_count} total)`);

      return response;
    } catch (error) {
      console.error('❌ Error recalling memories:', error);
      throw new Error(`Failed to recall memories: ${error.message}`);
    }
  }

  /**
   * Search memories with full-text search
   */
  async searchMemories(userId, query, limit = 50, offset = 0) {
    this.ensureInitialized();

    try {
      console.log(`🔍 Searching memories for user ${userId} with query "${query}"`);

      const result = this.rustLib.memex_search(
        this.handle,
        userId,
        query,
        limit,
        offset
      );

      if (!result) {
        return { data: [], total_count: 0, page: 0, per_page: limit, total_pages: 0, has_next: false, has_prev: false };
      }

      const response = JSON.parse(result);
      console.log(`✅ Search found ${response.data.length} memories`);

      return response;
    } catch (error) {
      console.error('❌ Error searching memories:', error);
      throw new Error(`Failed to search memories: ${error.message}`);
    }
  }

  /**
   * Get a memory by ID
   */
  async getMemory(memoryId) {
    this.ensureInitialized();

    try {
      const result = this.rustLib.memex_get_memory(this.handle, memoryId);

      if (!result) {
        return null;
      }

      const memory = JSON.parse(result);
      return memory;
    } catch (error) {
      console.error('❌ Error getting memory:', error);
      throw new Error(`Failed to get memory: ${error.message}`);
    }
  }

  /**
   * Update a memory
   */
  async updateMemory(memoryId, updates) {
    this.ensureInitialized();

    try {
      const updatesJson = JSON.stringify(updates);

      const success = this.rustLib.memex_update_memory(
        this.handle,
        memoryId,
        updatesJson
      );

      if (!success) {
        throw new Error('Memory not found or update failed');
      }

      console.log(`✅ Memory ${memoryId} updated successfully`);
      return true;
    } catch (error) {
      console.error('❌ Error updating memory:', error);
      throw new Error(`Failed to update memory: ${error.message}`);
    }
  }

  /**
   * Delete a memory
   */
  async deleteMemory(memoryId) {
    this.ensureInitialized();

    try {
      const success = this.rustLib.memex_delete_memory(this.handle, memoryId);

      if (!success) {
        throw new Error('Memory not found');
      }

      console.log(`✅ Memory ${memoryId} deleted successfully`);
      return true;
    } catch (error) {
      console.error('❌ Error deleting memory:', error);
      throw new Error(`Failed to delete memory: ${error.message}`);
    }
  }

  /**
   * Create a new session
   */
  async createSession(userId, name = null) {
    this.ensureInitialized();

    try {
      console.log(`📁 Creating session for user ${userId}`);

      const result = this.rustLib.memex_create_session(
        this.handle,
        userId,
        name
      );

      if (!result) {
        throw new Error('Failed to create session');
      }

      console.log(`✅ Created session ${result} for user ${userId}`);
      return result;
    } catch (error) {
      console.error('❌ Error creating session:', error);
      throw new Error(`Failed to create session: ${error.message}`);
    }
  }

  /**
   * Get user sessions
   */
  async getUserSessions(userId, limit = 50, offset = 0) {
    this.ensureInitialized();

    try {
      console.log(`📁 Getting sessions for user ${userId}`);

      const result = this.rustLib.memex_get_user_sessions(
        this.handle,
        userId,
        limit,
        offset
      );

      if (!result) {
        return { data: [], total_count: 0, page: 0, per_page: limit, total_pages: 0, has_next: false, has_prev: false };
      }

      const response = JSON.parse(result);
      console.log(`✅ Found ${response.data.length} sessions for user ${userId}`);

      return response;
    } catch (error) {
      console.error('❌ Error getting user sessions:', error);
      throw new Error(`Failed to get user sessions: ${error.message}`);
    }
  }

  /**
   * Generate session summary
   */
  async summarizeSession(sessionId) {
    this.ensureInitialized();

    try {
      console.log(`📋 Generating summary for session ${sessionId}`);

      const result = this.rustLib.memex_summarize_session(this.handle, sessionId);

      if (!result) {
        throw new Error('No summary generated');
      }

      const summary = JSON.parse(result);
      console.log(`✅ Summary generated for session ${sessionId}`);

      return summary;
    } catch (error) {
      console.error('❌ Error generating summary:', error);
      throw new Error(`Failed to generate summary: ${error.message}`);
    }
  }

  /**
   * Search sessions by keywords
   */
  async searchSessions(userId, keywords) {
    this.ensureInitialized();

    try {
      const keywordsJson = JSON.stringify(keywords);

      console.log(`🔍 Searching sessions for user ${userId} with keywords:`, keywords);

      const result = this.rustLib.memex_search_sessions(
        this.handle,
        userId,
        keywordsJson
      );

      if (!result) {
        return [];
      }

      const sessions = JSON.parse(result);
      console.log(`✅ Found ${sessions.length} matching sessions`);

      return sessions;
    } catch (error) {
      console.error('❌ Error searching sessions:', error);
      throw new Error(`Failed to search sessions: ${error.message}`);
    }
  }

  /**
   * Delete session
   */
  async deleteSession(sessionId, deleteMemories = false) {
    this.ensureInitialized();

    try {
      const success = this.rustLib.memex_delete_session(
        this.handle,
        sessionId,
        deleteMemories
      );

      if (!success) {
        throw new Error('Session not found');
      }

      console.log(`✅ Session ${sessionId} deleted successfully`);
      return { sessionId, memoriesDeleted: deleteMemories };
    } catch (error) {
      console.error('❌ Error deleting session:', error);
      throw new Error(`Failed to delete session: ${error.message}`);
    }
  }

  /**
   * Run memory decay process
   */
  async runDecay() {
    this.ensureInitialized();

    try {
      console.log('🧹 Running memory decay process');

      const result = this.rustLib.memex_decay(this.handle);

      if (!result) {
        throw new Error('No decay stats returned');
      }

      const decayStats = JSON.parse(result);
      console.log(`✅ Decay process completed - expired: ${decayStats.memories_expired}, compressed: ${decayStats.memories_compressed}`);

      return decayStats;
    } catch (error) {
      console.error('❌ Error running decay process:', error);
      throw new Error(`Failed to run decay process: ${error.message}`);
    }
  }

  /**
   * Get decay analysis
   */
  async analyzeDecay() {
    this.ensureInitialized();

    try {
      console.log('📊 Analyzing decay recommendations');

      const result = this.rustLib.memex_decay_analyze(this.handle);

      if (!result) {
        throw new Error('No decay analysis returned');
      }

      const analysis = JSON.parse(result);
      console.log(`✅ Decay analysis complete - ${analysis.total_memories} total memories`);

      return analysis;
    } catch (error) {
      console.error('❌ Error analyzing decay:', error);
      throw new Error(`Failed to analyze decay: ${error.message}`);
    }
  }

  /**
   * Get system statistics
   */
  async getStats() {
    this.ensureInitialized();

    try {
      console.log('📊 Getting system statistics');

      const result = this.rustLib.memex_get_stats(this.handle);

      if (!result) {
        return {};
      }

      const stats = JSON.parse(result);
      console.log('✅ Statistics retrieved');

      return stats;
    } catch (error) {
      console.error('❌ Error getting statistics:', error);
      throw new Error(`Failed to get statistics: ${error.message}`);
    }
  }

  /**
   * Export user memories
   */
  async exportUserMemories(userId) {
    this.ensureInitialized();

    try {
      console.log(`📤 Exporting memories for user ${userId}`);

      const result = this.rustLib.memex_export_user_memories(this.handle, userId);

      if (!result) {
        return '[]';
      }

      console.log(`✅ Exported memories for user ${userId}`);
      return result;
    } catch (error) {
      console.error('❌ Error exporting memories:', error);
      throw new Error(`Failed to export memories: ${error.message}`);
    }
  }

  /**
   * Get user memory statistics
   */
  async getUserStats(userId) {
    this.ensureInitialized();

    try {
      console.log(`📊 Getting statistics for user ${userId}`);

      const result = this.rustLib.memex_get_user_stats(this.handle, userId);

      if (!result) {
        return null;
      }

      const stats = JSON.parse(result);
      console.log(`✅ Retrieved statistics for user ${userId}`);

      return stats;
    } catch (error) {
      console.error('❌ Error getting user statistics:', error);
      throw new Error(`Failed to get user statistics: ${error.message}`);
    }
  }

  /**
   * Get session analytics
   */
  async getSessionAnalytics(userId) {
    this.ensureInitialized();

    try {
      console.log(`📊 Getting session analytics for user ${userId}`);

      const result = this.rustLib.memex_get_session_analytics(this.handle, userId);

      if (!result) {
        return null;
      }

      const analytics = JSON.parse(result);
      console.log(`✅ Retrieved session analytics for user ${userId}`);

      return analytics;
    } catch (error) {
      console.error('❌ Error getting session analytics:', error);
      throw new Error(`Failed to get session analytics: ${error.message}`);
    }
  }

  /**
   * Get library version
   */
  async getVersion() {
    if (!this.rustLib) {
      await this.loadRustLibrary();
    }

    try {
      const version = this.rustLib.memex_version();
      return version || 'unknown';
    } catch (error) {
      return 'unknown';
    }
  }

  /**
   * Ensure the bridge is initialized
   */
  ensureInitialized() {
    if (!this.isInitialized || !this.handle || this.handle === 0) {
      throw new Error('Rust bridge not initialized. Call initialize() first.');
    }
  }

  /**
   * Cleanup resources
   */
  async cleanup() {
    try {
      if (this.handle && this.handle !== 0) {
        console.log('🧹 Cleaning up Rust bridge...');
        this.rustLib.memex_destroy(this.handle);
        this.handle = 0;
      }

      this.isInitialized = false;
      console.log('✅ Rust bridge cleaned up');
    } catch (error) {
      console.error('❌ Error cleaning up Rust bridge:', error);
    }
  }

  /**
   * Health check
   */
  async healthCheck() {
    try {
      if (!this.isInitialized) {
        return { status: 'not_initialized' };
      }

      // Verify handle is still valid
      if (!this.rustLib.memex_is_valid(this.handle)) {
        return { status: 'invalid_handle' };
      }

      // Try to get stats as a health check
      await this.getStats();

      return {
        status: 'healthy',
        initialized: true,
        handle: this.handle,
        version: await this.getVersion(),
        config: this.config
      };
    } catch (error) {
      return {
        status: 'unhealthy',
        error: error.message,
        initialized: this.isInitialized,
        handle: this.handle
      };
    }
  }
}

module.exports = RustBridge;