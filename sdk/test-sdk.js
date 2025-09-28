// test-sdk.js
const { MindCacheSDK, MindCacheError } = require('./index.js');

// Initialize SDK
const mindcache = new MindCacheSDK({
    baseUrl: 'http://localhost:3000',
    timeout: 30000,
    debug: true  // Enable debug logging
});

async function playWithSDK() {
    try {
        console.log('🧠 Playing with MindCache SDK...\n');
        
        // Test 1: Check API connectivity
        console.log('1. Testing API connectivity...');
        const isAvailable = await mindcache.isApiAvailable();
        console.log(`API Available: ${isAvailable}\n`);
        
        if (!isAvailable) {
            console.log('❌ API not available. Make sure the server is running!');
            return;
        }
        
        // Test 2: Save some memories
        console.log('2. Saving memories...');
        const userId = `user_${Date.now()}`;
        const sessionId = `session_${Date.now()}`;
        
        const memory1 = await mindcache.saveMemory({
            userId,
            sessionId,
            content: "I learned about JavaScript async/await today",
            importance: 0.8,
            metadata: { category: "learning", language: "javascript" }
        });
        console.log('Memory 1 saved:', memory1.data.memoryId);
        
        const memory2 = await mindcache.saveMemory({
            userId,
            sessionId,
            content: "Discovered a great API design pattern",
            importance: 0.9,
            metadata: { category: "learning", topic: "api-design" }
        });
        console.log('Memory 2 saved:', memory2.data.memoryId);
        
        // Test 3: Recall memories
        console.log('\n3. Recalling memories...');
        const recalled = await mindcache.recallMemories({
            userId,
            limit: 10
        });
        console.log(`Found ${recalled.data.memories.length} memories:`);
        recalled.data.memories.forEach((mem, i) => {
            console.log(`  ${i+1}. ${mem.content} (importance: ${mem.importance})`);
        });
        
        // Test 4: Search with query
        console.log('\n4. Searching with query...');
        const searchResults = await mindcache.recallMemories({
            userId,
            query: "JavaScript",
            limit: 5
        });
        console.log(`Search results for "JavaScript": ${searchResults.data.memories.length} found`);
        
        // Test 5: Session management
        console.log('\n5. Testing session management...');
        const sessions = await mindcache.getUserSessions(userId);
        console.log(`User has ${sessions.data.sessions.length} sessions`);
        
        // Test 6: Get statistics
        console.log('\n6. Getting statistics...');
        const stats = await mindcache.getSystemStats();
        console.log('System stats:', {
            totalUsers: stats.data.storage?.total_users || 0,
            totalMemories: stats.data.storage?.total_memories || 0
        });
        
        console.log('\n✅ SDK test completed successfully!');
        
    } catch (error) {
        console.error('❌ Error:', error.message);
        if (error instanceof MindCacheError) {
            console.error('Status:', error.status);
        }
    }
}

// Run the test
playWithSDK();