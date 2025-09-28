const { MindCacheSDK } = require('../sdk');

class AIAgent {
    constructor(agentId) {
        this.agentId = agentId;
        this.mindcache = new MindCacheSDK({
            baseUrl: 'http://localhost:3000',
            debug: true
        });
        this.currentSession = null;
    }
    
    async startConversation(topic) {
        // Create a new session for this conversation
        const session = await this.mindcache.createSession({
            userId: this.agentId,
            name: `Conversation about ${topic}`,
            metadata: { topic, startTime: new Date().toISOString() }
        });
        
        this.currentSession = session.data.sessionId;
        console.log(`🤖 Started conversation: ${this.currentSession}`);
        return this.currentSession;
    }
    
    async remember(content, importance = 0.5, metadata = {}) {
        if (!this.currentSession) {
            throw new Error('No active session. Start a conversation first.');
        }
        
        const memory = await this.mindcache.saveMemory({
            userId: this.agentId,
            sessionId: this.currentSession,
            content,
            importance,
            metadata: {
                ...metadata,
                timestamp: new Date().toISOString(),
                source: 'ai-agent'
            }
        });
        
        console.log(`🧠 Remembered: ${content.substring(0, 50)}...`);
        return memory;
    }
    
    async recall(query, limit = 5) {
        const memories = await this.mindcache.recallMemories({
            userId: this.agentId,
            query,
            limit,
            minImportance: 0.3
        });
        
        console.log(`🔍 Recalled ${memories.data.memories.length} relevant memories`);
        return memories.data.memories;
    }
    
    async getContext(query) {
        // Get recent memories and relevant memories
        const [recent, relevant] = await Promise.all([
            this.mindcache.recallMemories({
                userId: this.agentId,
                sessionId: this.currentSession,
                limit: 3
            }),
            this.mindcache.recallMemories({
                userId: this.agentId,
                query,
                limit: 5,
                minImportance: 0.5
            })
        ]);
        
        return {
            recent: recent.data.memories,
            relevant: relevant.data.memories
        };
    }
    
    async summarizeConversation() {
        if (!this.currentSession) return null;
        
        const summary = await this.mindcache.summarizeSession(this.currentSession);
        console.log(`📋 Session summary generated`);
        return summary.data.summary;
    }
}

// Demo usage
async function demoAIAgent() {
    const agent = new AIAgent('ai_assistant_001');
    
    try {
        // Start a conversation
        await agent.startConversation('Machine Learning');
        
        // Remember some interactions
        await agent.remember(
            "User asked about neural network architectures",
            0.8,
            { category: 'question', topic: 'neural-networks' }
        );
        
        await agent.remember(
            "Explained the difference between CNN and RNN",
            0.9,
            { category: 'explanation', topic: 'neural-networks' }
        );
        
        await agent.remember(
            "User seems interested in practical applications",
            0.7,
            { category: 'observation', sentiment: 'interested' }
        );
        
        // Recall relevant context
        const context = await agent.getContext('neural networks');
        console.log('\n📚 Context for next response:');
        console.log('Recent:', context.recent.map(m => m.content));
        console.log('Relevant:', context.relevant.map(m => m.content));
        
        // Generate summary
        const summary = await agent.summarizeConversation();
        console.log('\n📋 Conversation Summary:', summary);
        
    } catch (error) {
        console.error('❌ Demo error:', error.message);
    }
}

// Run the demo
demoAIAgent();