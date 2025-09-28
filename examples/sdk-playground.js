const { MindCacheSDK } = require('../sdk');
const readline = require('readline');

const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
});

const mindcache = new MindCacheSDK({
    baseUrl: 'http://localhost:3000',
    debug: true
});

function prompt(question) {
    return new Promise(resolve => {
        rl.question(question, resolve);
    });
}

async function sdkPlayground() {
    console.log('🧠 MindCache SDK Playground\n');
    console.log('Available commands:');
    console.log('1. save - Save a memory');
    console.log('2. recall - Recall memories');
    console.log('3. search - Search memories');
    console.log('4. sessions - List sessions');
    console.log('5. stats - Get statistics');
    console.log('6. export - Export memories');
    console.log('7. bulk - Bulk save memories');
    console.log('8. health - Check health');
    console.log('9. exit - Exit playground\n');
    
    while (true) {
        try {
            const command = await prompt('Enter command (1-9): ');
            
            switch (command.trim()) {
                case '1':
                case 'save':
                    await handleSave();
                    break;
                    
                case '2':
                case 'recall':
                    await handleRecall();
                    break;
                    
                case '3':
                case 'search':
                    await handleSearch();
                    break;
                    
                case '4':
                case 'sessions':
                    await handleSessions();
                    break;
                    
                case '5':
                case 'stats':
                    await handleStats();
                    break;
                    
                case '6':
                case 'export':
                    await handleExport();
                    break;
                    
                case '7':
                case 'bulk':
                    await handleBulk();
                    break;
                    
                case '8':
                case 'health':
                    await handleHealth();
                    break;
                    
                case '9':
                case 'exit':
                    console.log('👋 Goodbye!');
                    rl.close();
                    return;
                    
                default:
                    console.log('❌ Invalid command. Try again.\n');
            }
            
        } catch (error) {
            console.error('❌ Error:', error.message);
        }
        
        console.log('\n' + '─'.repeat(50) + '\n');
    }
}

async function handleSave() {
    const userId = await prompt('User ID: ');
    const sessionId = await prompt('Session ID: ');
    const content = await prompt('Memory content: ');
    const importance = parseFloat(await prompt('Importance (0-1, default 0.5): ') || '0.5');
    
    const result = await mindcache.saveMemory({
        userId,
        sessionId,
        content,
        importance
    });
    
    console.log('✅ Saved:', result.data.memoryId);
}

async function handleRecall() {
    const userId = await prompt('User ID: ');
    const query = await prompt('Search query (optional): ');
    const limit = parseInt(await prompt('Limit (default 10): ') || '10');
    
    const result = await mindcache.recallMemories({
        userId,
        query: query || undefined,
        limit
    });
    
    console.log(`📚 Found ${result.data.memories.length} memories:`);
    result.data.memories.forEach((mem, i) => {
        console.log(`${i+1}. ${mem.content} (${mem.importance})`);
    });
}

async function handleSearch() {
    const userId = await prompt('User ID: ');
    const searchText = await prompt('Search text: ');
    
    const results = await mindcache.findMemories(userId, searchText);
    
    console.log(`🔍 Search results: ${results.data.memories.length} found`);
    results.data.memories.forEach((mem, i) => {
        console.log(`${i+1}. ${mem.content}`);
    });
}

async function handleSessions() {
    const userId = await prompt('User ID: ');
    
    const sessions = await mindcache.getUserSessions(userId);
    
    console.log(`📁 Sessions for ${userId}: ${sessions.data.sessions.length}`);
    sessions.data.sessions.forEach((session, i) => {
        console.log(`${i+1}. ${session.name || session.id} (${session.memoryCount} memories)`);
    });
}

async function handleStats() {
    const stats = await mindcache.getSystemStats();
    console.log('📊 System Statistics:', JSON.stringify(stats.data, null, 2));
}

async function handleExport() {
    const userId = await prompt('User ID: ');
    
    const exported = await mindcache.exportMemories(userId);
    console.log(`📤 Exported ${exported.data.memories.length} memories for ${userId}`);
}

async function handleBulk() {
    const userId = await prompt('User ID: ');
    const sessionId = await prompt('Session ID: ');
    
    const memories = [
        { content: 'Bulk memory 1', importance: 0.6 },
        { content: 'Bulk memory 2', importance: 0.7 },
        { content: 'Bulk memory 3', importance: 0.8 }
    ].map(mem => ({ ...mem, userId, sessionId }));
    
    const result = await mindcache.bulkSaveMemories(memories);
    console.log('📦 Bulk save result:', result.data.summary);
}

async function handleHealth() {
    const health = await mindcache.getHealth();
    console.log('🏥 Health:', health.data.status);
}

// Start the playground
sdkPlayground().catch(console.error);