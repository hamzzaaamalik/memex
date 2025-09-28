/**
 * MindCache JavaScript SDK
 * 
 * A lightweight client library for interacting with MindCache API
 * Provides an easy-to-use interface for memory storage and retrieval
 */

class MindCacheSDK {
    constructor(options = {}) {
        this.baseUrl = options.baseUrl || 'http://localhost:3000';
        this.apiKey = options.apiKey || null;
        this.timeout = options.timeout || 30000;
        this.retries = options.retries || 3;
        this.debug = options.debug || false;
        
        // Default headers
        this.defaultHeaders = {
            'Content-Type': 'application/json',
            'User-Agent': 'MindCache-SDK/1.0.0'
        };
        
        if (this.apiKey) {
            this.defaultHeaders['X-API-Key'] = this.apiKey;
        }
        
        this.log('🧠 MindCache SDK initialized', { baseUrl: this.baseUrl });
    }

    /**
     * Memory Operations
     */

    /**
     * Save a memory
     * @param {Object} memory - Memory object
     * @param {string} memory.userId - User identifier
     * @param {string} memory.sessionId - Session identifier
     * @param {string} memory.content - Memory content
     * @param {Object} [memory.metadata] - Additional metadata
     * @param {number} [memory.importance] - Importance score (0-1)
     * @param {number} [memory.ttlHours] - Time to live in hours
     * @returns {Promise<Object>} Save result with memory ID
     */
    async saveMemory(memory) {
        this.log('💾 Saving memory', { userId: memory.userId, sessionId: memory.sessionId });
        
        const response = await this.request('POST', '/api/memory/save', memory);
        
        this.log('✅ Memory saved', { memoryId: response.data.memoryId });
        return response;
    }

    /**
     * Recall memories with filters
     * @param {Object} filter - Filter options
     * @param {string} filter.userId - User identifier
     * @param {string} [filter.query] - Search query
     * @param {string} [filter.sessionId] - Session identifier
     * @param {string} [filter.dateFrom] - Start date (ISO string)
     * @param {string} [filter.dateTo] - End date (ISO string)
     * @param {number} [filter.limit] - Maximum results
     * @param {number} [filter.minImportance] - Minimum importance score
     * @param {string[]} [filter.keywords] - Keywords to search for
     * @returns {Promise<Object>} Recall result with memories array
     */
    async recallMemories(filter) {
        this.log('🔍 Recalling memories', { userId: filter.userId, query: filter.query });
        
        const response = await this.request('POST', '/api/memory/recall', filter);
        
        this.log('✅ Memories recalled', { count: response.data.count });
        return response;
    }

    /**
     * Search memories using GET endpoint (for URL-based searches)
     * @param {Object} params - Search parameters
     * @returns {Promise<Object>} Search results
     */
    async searchMemories(params) {
        this.log('🔍 Searching memories', params);
        
        const queryString = new URLSearchParams(params).toString();
        const response = await this.request('GET', `/api/memory/search?${queryString}`);
        
        this.log('✅ Search completed', { count: response.data.count });
        return response;
    }

    /**
     * Generate a session summary
     * @param {string} sessionId - Session identifier
     * @returns {Promise<Object>} Session summary
     */
    async summarizeSession(sessionId) {
        this.log('📋 Generating session summary', { sessionId });
        
        const response = await this.request('POST', '/api/memory/summarize', { sessionId });
        
        this.log('✅ Summary generated', { sessionId });
        return response;
    }

    /**
     * Export user memories
     * @param {string} userId - User identifier
     * @returns {Promise<Object>} Exported memories
     */
    async exportMemories(userId) {
        this.log('📤 Exporting memories', { userId });
        
        const response = await this.request('GET', `/api/memory/export/${userId}`);
        
        this.log('✅ Memories exported', { userId });
        return response;
    }

    /**
     * Run memory decay process
     * @param {boolean} [force=false] - Force decay process
     * @returns {Promise<Object>} Decay statistics
     */
    async runDecay(force = false) {
        this.log('🧹 Running memory decay', { force });
        
        const response = await this.request('DELETE', '/api/memory/decay', { force });
        
        this.log('✅ Decay completed', response.data.decayStats);
        return response;
    }

    /**
     * Bulk save multiple memories
     * @param {Object[]} memories - Array of memory objects
     * @returns {Promise<Object>} Bulk save results
     */
    async bulkSaveMemories(memories) {
        this.log('📦 Bulk saving memories', { count: memories.length });
        
        const response = await this.request('POST', '/api/memory/bulk', { memories });
        
        this.log('✅ Bulk save completed', response.data.summary);
        return response;
    }

    /**
     * Session Operations
     */

    /**
     * Create a new session
     * @param {Object} session - Session object
     * @param {string} session.userId - User identifier
     * @param {string} [session.name] - Session name
     * @param {Object} [session.metadata] - Session metadata
     * @returns {Promise<Object>} Created session
     */
    async createSession(session) {
        this.log('📁 Creating session', { userId: session.userId, name: session.name });
        
        const response = await this.request('POST', '/api/sessions', session);
        
        this.log('✅ Session created', { sessionId: response.data.sessionId });
        return response;
    }

    /**
     * Get user sessions
     * @param {string} userId - User identifier
     * @returns {Promise<Object>} User sessions
     */
    async getUserSessions(userId) {
        this.log('📂 Getting user sessions', { userId });
        
        const response = await this.request('GET', `/api/sessions/${userId}`);
        
        this.log('✅ Sessions retrieved', { count: response.data.count });
        return response;
    }

    /**
     * Get a specific session
     * @param {string} userId - User identifier
     * @param {string} sessionId - Session identifier
     * @returns {Promise<Object>} Session details
     */
    async getSession(userId, sessionId) {
        this.log('📄 Getting session', { userId, sessionId });
        
        const response = await this.request('GET', `/api/sessions/${userId}/${sessionId}`);
        
        this.log('✅ Session retrieved', { sessionId });
        return response;
    }

    /**
     * Get session memories
     * @param {string} userId - User identifier
     * @param {string} sessionId - Session identifier
     * @param {Object} [options] - Options (limit, offset)
     * @returns {Promise<Object>} Session memories
     */
    async getSessionMemories(userId, sessionId, options = {}) {
        this.log('📋 Getting session memories', { userId, sessionId });
        
        let url = `/api/sessions/${userId}/${sessionId}/memories`;
        if (options.limit || options.offset) {
            const params = new URLSearchParams(options).toString();
            url += `?${params}`;
        }
        
        const response = await this.request('GET', url);
        
        this.log('✅ Session memories retrieved', { count: response.data.memories.length });
        return response;
    }

    /**
     * Search sessions by keywords
     * @param {string} userId - User identifier
     * @param {string[]} keywords - Keywords to search for
     * @returns {Promise<Object>} Matching sessions
     */
    async searchSessions(userId, keywords) {
        this.log('🔍 Searching sessions', { userId, keywords });
        
        const response = await this.request('POST', '/api/sessions/search', { userId, keywords });
        
        this.log('✅ Session search completed', { count: response.data.count });
        return response;
    }

    /**
     * Update session metadata
     * @param {string} userId - User identifier
     * @param {string} sessionId - Session identifier
     * @param {Object} updates - Updates to apply
     * @returns {Promise<Object>} Updated session
     */
    async updateSession(userId, sessionId, updates) {
        this.log('✏️ Updating session', { userId, sessionId });
        
        const response = await this.request('PUT', `/api/sessions/${userId}/${sessionId}`, updates);
        
        this.log('✅ Session updated', { sessionId });
        return response;
    }

    /**
     * Delete a session
     * @param {string} userId - User identifier
     * @param {string} sessionId - Session identifier
     * @param {boolean} [confirm=false] - Confirmation flag
     * @returns {Promise<Object>} Deletion result
     */
    async deleteSession(userId, sessionId, confirm = false) {
        this.log('🗑️ Deleting session', { userId, sessionId, confirm });
        
        let url = `/api/sessions/${userId}/${sessionId}`;
        if (confirm) {
            url += '?confirm=true';
        }
        
        const response = await this.request('DELETE', url);
        
        this.log('✅ Session deleted', { sessionId, memoriesDeleted: response.data.memoriesDeleted });
        return response;
    }

    /**
     * Statistics Operations
     */

    /**
     * Get system statistics
     * @returns {Promise<Object>} System statistics
     */
    async getSystemStats() {
        this.log('📊 Getting system stats');
        
        const response = await this.request('GET', '/api/stats');
        
        this.log('✅ System stats retrieved');
        return response;
    }

    /**
     * Get user statistics
     * @param {string} userId - User identifier
     * @returns {Promise<Object>} User statistics
     */
    async getUserStats(userId) {
        this.log('📊 Getting user stats', { userId });
        
        const response = await this.request('GET', `/api/stats/user/${userId}`);
        
        this.log('✅ User stats retrieved', { userId });
        return response;
    }

    /**
     * Get memory statistics
     * @returns {Promise<Object>} Memory statistics
     */
    async getMemoryStats() {
        this.log('📊 Getting memory stats');
        
        const response = await this.request('GET', '/api/stats/memory');
        
        this.log('✅ Memory stats retrieved');
        return response;
    }

    /**
     * Get health status
     * @returns {Promise<Object>} Health status
     */
    async getHealth() {
        this.log('🏥 Getting health status');
        
        const response = await this.request('GET', '/api/stats/health');
        
        this.log('✅ Health status retrieved', { status: response.data.status });
        return response;
    }

    /**
     * Utility Methods
     */

    /**
     * Check if the API is available
     * @returns {Promise<boolean>} API availability status
     */
    async isApiAvailable() {
        try {
            await this.request('GET', '/health');
            return true;
        } catch (error) {
            this.log('❌ API not available', { error: error.message });
            return false;
        }
    }

    /**
     * Get API information
     * @returns {Promise<Object>} API information
     */
    async getApiInfo() {
        this.log('ℹ️ Getting API info');
        
        const response = await this.request('GET', '/api');
        
        this.log('✅ API info retrieved');
        return response;
    }

    /**
     * Wait for API to become available
     * @param {number} [maxWaitTime=30000] - Maximum wait time in milliseconds
     * @param {number} [checkInterval=1000] - Check interval in milliseconds
     * @returns {Promise<boolean>} Whether API became available
     */
    async waitForApi(maxWaitTime = 30000, checkInterval = 1000) {
        this.log('⏳ Waiting for API to become available');
        
        const startTime = Date.now();
        
        while (Date.now() - startTime < maxWaitTime) {
            if (await this.isApiAvailable()) {
                this.log('✅ API is now available');
                return true;
            }
            
            await new Promise(resolve => setTimeout(resolve, checkInterval));
        }
        
        this.log('❌ API did not become available within timeout');
        return false;
    }

    /**
     * Core HTTP Request Method
     */

    /**
     * Make an HTTP request with retry logic
     * @param {string} method - HTTP method
     * @param {string} path - API path
     * @param {Object} [data] - Request data
     * @param {Object} [options] - Additional options
     * @returns {Promise<Object>} Response data
     */
    async request(method, path, data = null, options = {}) {
        const url = `${this.baseUrl}${path}`;
        const headers = { ...this.defaultHeaders, ...options.headers };
        
        const requestOptions = {
            method,
            headers,
            ...options
        };
        
        if (data && (method === 'POST' || method === 'PUT' || method === 'PATCH')) {
            requestOptions.body = JSON.stringify(data);
        }
        
        let lastError;
        
        for (let attempt = 1; attempt <= this.retries; attempt++) {
            try {
                this.log(`🌐 ${method} ${path} (attempt ${attempt}/${this.retries})`);
                
                const response = await this.fetchWithTimeout(url, requestOptions);
                
                if (!response.ok) {
                    const errorData = await response.text();
                    let errorMessage;
                    
                    try {
                        const parsed = JSON.parse(errorData);
                        errorMessage = parsed.error || parsed.message || `HTTP ${response.status}`;
                    } catch {
                        errorMessage = errorData || `HTTP ${response.status}`;
                    }
                    
                    throw new MindCacheError(errorMessage, response.status, errorData);
                }
                
                const responseData = await response.json();
                this.log(`✅ ${method} ${path} completed`);
                
                return responseData;
                
            } catch (error) {
                lastError = error;
                
                this.log(`❌ ${method} ${path} failed (attempt ${attempt}): ${error.message}`);
                
                // Don't retry on client errors (4xx)
                if (error.status && error.status >= 400 && error.status < 500) {
                    break;
                }
                
                // Wait before retrying (exponential backoff)
                if (attempt < this.retries) {
                    const delay = Math.min(1000 * Math.pow(2, attempt - 1), 10000);
                    this.log(`⏳ Retrying in ${delay}ms...`);
                    await new Promise(resolve => setTimeout(resolve, delay));
                }
            }
        }
        
        throw lastError;
    }

    /**
     * Fetch with timeout support
     * @param {string} url - Request URL
     * @param {Object} options - Fetch options
     * @returns {Promise<Response>} Fetch response
     */
    async fetchWithTimeout(url, options) {
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), this.timeout);
        
        try {
            const response = await fetch(url, {
                ...options,
                signal: controller.signal
            });
            
            return response;
        } catch (error) {
            if (error.name === 'AbortError') {
                throw new MindCacheError(`Request timeout after ${this.timeout}ms`, 0);
            }
            throw error;
        } finally {
            clearTimeout(timeoutId);
        }
    }

    /**
     * Debug logging
     * @param {string} message - Log message
     * @param {Object} [data] - Additional data
     */
    log(message, data = null) {
        if (this.debug) {
            if (data) {
                console.log(`[MindCache SDK] ${message}`, data);
            } else {
                console.log(`[MindCache SDK] ${message}`);
            }
        }
    }

    /**
     * Convenience Methods for Common Use Cases
     */

    /**
     * Save a simple text memory
     * @param {string} userId - User identifier
     * @param {string} sessionId - Session identifier
     * @param {string} content - Memory content
     * @param {number} [importance=0.5] - Importance score
     * @returns {Promise<Object>} Save result
     */
    async saveText(userId, sessionId, content, importance = 0.5) {
        return this.saveMemory({
            userId,
            sessionId,
            content,
            importance
        });
    }

    /**
     * Search for memories containing specific text
     * @param {string} userId - User identifier
     * @param {string} searchText - Text to search for
     * @param {number} [limit=10] - Maximum results
     * @returns {Promise<Object>} Search results
     */
    async findMemories(userId, searchText, limit = 10) {
        return this.recallMemories({
            userId,
            query: searchText,
            limit
        });
    }

    /**
     * Get all memories for a user
     * @param {string} userId - User identifier
     * @param {number} [limit=100] - Maximum results
     * @returns {Promise<Object>} All user memories
     */
    async getAllMemories(userId, limit = 100) {
        return this.recallMemories({
            userId,
            limit
        });
    }

    /**
     * Create a session and save the first memory
     * @param {string} userId - User identifier
     * @param {string} sessionName - Session name
     * @param {string} firstMemory - First memory content
     * @returns {Promise<Object>} Session and memory creation results
     */
    async startSession(userId, sessionName, firstMemory) {
        const sessionResult = await this.createSession({
            userId,
            name: sessionName
        });
        
        const memoryResult = await this.saveMemory({
            userId,
            sessionId: sessionResult.data.sessionId,
            content: firstMemory
        });
        
        return {
            session: sessionResult,
            memory: memoryResult
        };
    }
}

/**
 * Custom Error Class for MindCache SDK
 */
class MindCacheError extends Error {
    constructor(message, status = 0, response = null) {
        super(message);
        this.name = 'MindCacheError';
        this.status = status;
        this.response = response;
    }
}

// Export for different environments
if (typeof module !== 'undefined' && module.exports) {
    // Node.js
    module.exports = { MindCacheSDK, MindCacheError };
} else if (typeof define === 'function' && define.amd) {
    // AMD
    define([], function() {
        return { MindCacheSDK, MindCacheError };
    });
} else {
    // Browser global
    window.MindCacheSDK = MindCacheSDK;
    window.MindCacheError = MindCacheError;
}