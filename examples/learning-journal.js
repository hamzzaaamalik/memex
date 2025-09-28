const { MindCacheSDK } = require('../sdk');

class LearningJournal {
    constructor(studentId) {
        this.studentId = studentId;
        this.mindcache = new MindCacheSDK({
            baseUrl: 'http://localhost:3000'
        });
    }
    
    async logLearning(subject, content, difficulty = 0.5) {
        const importance = this.calculateImportance(difficulty, content);
        
        const memory = await this.mindcache.saveMemory({
            userId: this.studentId,
            sessionId: `study_${subject.toLowerCase().replace(/\s+/g, '_')}`,
            content,
            importance,
            metadata: {
                subject,
                difficulty,
                learningDate: new Date().toISOString(),
                type: 'learning_note'
            }
        });
        
        console.log(`📝 Logged learning: ${subject}`);
        return memory;
    }
    
    calculateImportance(difficulty, content) {
        let importance = 0.5;
        
        // Higher difficulty = higher importance
        importance += (difficulty - 0.5) * 0.3;
        
        // Longer content = higher importance
        if (content.length > 100) importance += 0.1;
        if (content.length > 300) importance += 0.1;
        
        // Cap between 0 and 1
        return Math.max(0, Math.min(1, importance));
    }
    
    async reviewSubject(subject) {
        const memories = await this.mindcache.recallMemories({
            userId: this.studentId,
            query: subject,
            limit: 10
        });
        
        console.log(`\n📚 Review: ${subject}`);
        console.log(`Found ${memories.data.memories.length} related notes:\n`);
        
        memories.data.memories.forEach((memory, i) => {
            console.log(`${i+1}. ${memory.content}`);
            console.log(`   Difficulty: ${memory.metadata?.difficulty || 'N/A'}`);
            console.log(`   Date: ${new Date(memory.timestamp).toLocaleDateString()}\n`);
        });
        
        return memories.data.memories;
    }
    
    async getWeakAreas() {
        // Find topics with high difficulty but low understanding
        const allMemories = await this.mindcache.getAllMemories(this.studentId, 100);
        
        const subjectStats = {};
        
        allMemories.data.memories.forEach(memory => {
            const subject = memory.metadata?.subject || 'Unknown';
            const difficulty = memory.metadata?.difficulty || 0.5;
            
            if (!subjectStats[subject]) {
                subjectStats[subject] = { 
                    totalDifficulty: 0, 
                    count: 0, 
                    avgDifficulty: 0 
                };
            }
            
            subjectStats[subject].totalDifficulty += difficulty;
            subjectStats[subject].count++;
            subjectStats[subject].avgDifficulty = 
                subjectStats[subject].totalDifficulty / subjectStats[subject].count;
        });
        
        // Sort by difficulty
        const weakAreas = Object.entries(subjectStats)
            .filter(([_, stats]) => stats.avgDifficulty > 0.7)
            .sort((a, b) => b[1].avgDifficulty - a[1].avgDifficulty);
        
        console.log('\n⚠️  Areas needing more attention:');
        weakAreas.forEach(([subject, stats]) => {
            console.log(`${subject}: ${stats.avgDifficulty.toFixed(2)} difficulty (${stats.count} notes)`);
        });
        
        return weakAreas;
    }
}

// Demo usage
async function demoLearningJournal() {
    const journal = new LearningJournal('student_alice');
    
    try {
        // Log some learning
        await journal.logLearning(
            'Mathematics',
            'Learned about calculus derivatives and their applications in optimization',
            0.8
        );
        
        await journal.logLearning(
            'Programming',
            'Understood how async/await works in JavaScript with practical examples',
            0.6
        );
        
        await journal.logLearning(
            'Mathematics',
            'Practiced integration by parts - still struggling with complex problems',
            0.9
        );
        
        // Review a subject
        await journal.reviewSubject('Mathematics');
        
        // Check weak areas
        await journal.getWeakAreas();
        
    } catch (error) {
        console.error('❌ Journal error:', error.message);
    }
}

// Run the demo
demoLearningJournal();