#!/usr/bin/env node

/**
 * MindCache CLI - Command Line Interface
 * 
 * A developer-friendly CLI tool for testing and managing MindCache
 * Supports all core operations with interactive prompts and formatting
 */

const { program } = require('commander');
const inquirer = require('inquirer');
const chalk = require('chalk');
const ora = require('ora');
const Table = require('cli-table3');
const fs = require('fs');
const path = require('path');
const { MindCacheSDK } = require('../sdk');

// Package info
const packageJson = require('./package.json');

// Default configuration
const DEFAULT_CONFIG = {
    baseUrl: 'http://localhost:3000',
    timeout: 30000,
    debug: false
};

// Global SDK instance
let sdk = null;

/**
 * Initialize SDK with configuration
 */
function initializeSDK(options = {}) {
    const config = { ...DEFAULT_CONFIG, ...options };
    
    sdk = new MindCacheSDK({
        baseUrl: config.baseUrl,
        apiKey: config.apiKey,
        timeout: config.timeout,
        debug: config.debug
    });
    
    return sdk;
}

/**
 * Utility Functions
 */

function formatOutput(data, format = 'table') {
    if (format === 'json') {
        return JSON.stringify(data, null, 2);
    }
    return data;
}

function handleError(error, spinner = null) {
    if (spinner) spinner.fail();
    
    console.log(chalk.red('\n❌ Error:'), error.message);
    
    if (error.status) {
        console.log(chalk.gray(`Status: ${error.status}`));
    }
    
    if (program.opts().debug && error.stack) {
        console.log(chalk.gray('\nStack trace:'));
        console.log(chalk.gray(error.stack));
    }
    
    process.exit(1);
}

function displaySuccess(message, data = null) {
    console.log(chalk.green('\n✅ Success:'), message);
    
    if (data) {
        console.log('\n' + chalk.blue('Result:'));
        if (typeof data === 'object') {
            console.log(formatOutput(data, program.opts().format));
        } else {
            console.log(data);
        }
    }
}

/**
 * Memory Commands
 */

program
    .name('mindcache')
    .description('MindCache CLI - Memory engine for AI applications')
    .version(packageJson.version)
    .option('-u, --url <url>', 'API base URL', DEFAULT_CONFIG.baseUrl)
    .option('-k, --api-key <key>', 'API key for authentication')
    .option('-f, --format <format>', 'Output format (json|table)', 'table')
    .option('-d, --debug', 'Enable debug mode', false)
    .option('-t, --timeout <ms>', 'Request timeout in milliseconds', DEFAULT_CONFIG.timeout);

// Save memory command
program
    .command('save')
    .description('Save a memory')
    .option('--user <userId>', 'User ID')
    .option('--session <sessionId>', 'Session ID')
    .option('--content <text>', 'Memory content')
    .option('--importance <score>', 'Importance score (0-1)', parseFloat)
    .option('--ttl <hours>', 'Time to live in hours', parseInt)
    .option('--metadata <json>', 'Metadata as JSON string')
    .option('-i, --interactive', 'Interactive mode')
    .action(async (options) => {
        try {
            sdk = initializeSDK(program.opts());
            
            let memoryData;
            
            if (options.interactive || !options.user || !options.session || !options.content) {
                // Interactive mode
                console.log(chalk.blue('\n📝 Save Memory - Interactive Mode\n'));
                
                const answers = await inquirer.prompt([
                    {
                        type: 'input',
                        name: 'userId',
                        message: 'User ID:',
                        default: options.user,
                        validate: input => input.trim() !== '' || 'User ID is required'
                    },
                    {
                        type: 'input',
                        name: 'sessionId',
                        message: 'Session ID:',
                        default: options.session || `session_${Date.now()}`,
                        validate: input => input.trim() !== '' || 'Session ID is required'
                    },
                    {
                        type: 'editor',
                        name: 'content',
                        message: 'Memory content (opens editor):',
                        default: options.content
                    },
                    {
                        type: 'number',
                        name: 'importance',
                        message: 'Importance (0-1):',
                        default: options.importance || 0.5,
                        validate: input => (input >= 0 && input <= 1) || 'Importance must be between 0 and 1'
                    },
                    {
                        type: 'number',
                        name: 'ttlHours',
                        message: 'TTL in hours (optional):',
                        default: options.ttl
                    },
                    {
                        type: 'input',
                        name: 'metadata',
                        message: 'Metadata (JSON format, optional):',
                        default: options.metadata
                    }
                ]);
                
                memoryData = {
                    userId: answers.userId,
                    sessionId: answers.sessionId,
                    content: answers.content,
                    importance: answers.importance
                };
                
                if (answers.ttlHours) memoryData.ttlHours = answers.ttlHours;
                if (answers.metadata) {
                    try {
                        memoryData.metadata = JSON.parse(answers.metadata);
                    } catch (error) {
                        throw new Error('Invalid JSON in metadata');
                    }
                }
            } else {
                // Command line mode
                memoryData = {
                    userId: options.user,
                    sessionId: options.session,
                    content: options.content
                };
                
                if (options.importance !== undefined) memoryData.importance = options.importance;
                if (options.ttl) memoryData.ttlHours = options.ttl;
                if (options.metadata) {
                    try {
                        memoryData.metadata = JSON.parse(options.metadata);
                    } catch (error) {
                        throw new Error('Invalid JSON in metadata');
                    }
                }
            }
            
            const spinner = ora('Saving memory...').start();
            
            const result = await sdk.saveMemory(memoryData);
            
            spinner.succeed();
            displaySuccess('Memory saved successfully', {
                memoryId: result.data.memoryId,
                userId: result.data.userId,
                sessionId: result.data.sessionId
            });
            
        } catch (error) {
            handleError(error);
        }
    });

// Recall memories command
program
    .command('recall')
    .description('Recall memories')
    .option('--user <userId>', 'User ID')
    .option('--query <text>', 'Search query')
    .option('--session <sessionId>', 'Session ID')
    .option('--limit <number>', 'Maximum results', parseInt, 10)
    .option('--importance <score>', 'Minimum importance', parseFloat)
    .option('--from <date>', 'Start date (ISO format)')
    .option('--to <date>', 'End date (ISO format)')
    .option('-i, --interactive', 'Interactive mode')
    .action(async (options) => {
        try {
            sdk = initializeSDK(program.opts());
            
            let filter;
            
            if (options.interactive || !options.user) {
                // Interactive mode
                console.log(chalk.blue('\n🔍 Recall Memories - Interactive Mode\n'));
                
                const answers = await inquirer.prompt([
                    {
                        type: 'input',
                        name: 'userId',
                        message: 'User ID:',
                        default: options.user,
                        validate: input => input.trim() !== '' || 'User ID is required'
                    },
                    {
                        type: 'input',
                        name: 'query',
                        message: 'Search query (optional):',
                        default: options.query
                    },
                    {
                        type: 'input',
                        name: 'sessionId',
                        message: 'Session ID (optional):',
                        default: options.session
                    },
                    {
                        type: 'number',
                        name: 'limit',
                        message: 'Maximum results:',
                        default: options.limit || 10
                    },
                    {
                        type: 'number',
                        name: 'minImportance',
                        message: 'Minimum importance (optional):',
                        default: options.importance
                    }
                ]);
                
                filter = {
                    userId: answers.userId,
                    limit: answers.limit
                };
                
                if (answers.query) filter.query = answers.query;
                if (answers.sessionId) filter.sessionId = answers.sessionId;
                if (answers.minImportance !== undefined) filter.minImportance = answers.minImportance;
            } else {
                // Command line mode
                filter = {
                    userId: options.user,
                    limit: options.limit || 10
                };
                
                if (options.query) filter.query = options.query;
                if (options.session) filter.sessionId = options.session;
                if (options.importance !== undefined) filter.minImportance = options.importance;
                if (options.from) filter.dateFrom = options.from;
                if (options.to) filter.dateTo = options.to;
            }
            
            const spinner = ora('Recalling memories...').start();
            
            const result = await sdk.recallMemories(filter);
            
            spinner.succeed();
            
            if (result.data.memories.length === 0) {
                console.log(chalk.yellow('\n📭 No memories found matching the criteria'));
                return;
            }
            
            console.log(chalk.green(`\n✅ Found ${result.data.memories.length} memories:\n`));
            
            if (program.opts().format === 'json') {
                console.log(JSON.stringify(result.data.memories, null, 2));
            } else {
                const table = new Table({
                    head: ['ID', 'Content', 'Importance', 'Session', 'Date'],
                    colWidths: [15, 50, 12, 20, 20]
                });
                
                result.data.memories.forEach(memory => {
                    table.push([
                        memory.id.substring(0, 12) + '...',
                        memory.content.length > 45 ? memory.content.substring(0, 45) + '...' : memory.content,
                        memory.importance?.toFixed(2) || 'N/A',
                        memory.sessionId.substring(0, 17) + '...',
                        new Date(memory.timestamp).toLocaleDateString()
                    ]);
                });
                
                console.log(table.toString());
            }
            
        } catch (error) {
            handleError(error);
        }
    });

// Summarize session command
program
    .command('summarize')
    .description('Generate session summary')
    .option('--session <sessionId>', 'Session ID')
    .option('-i, --interactive', 'Interactive mode')
    .action(async (options) => {
        try {
            sdk = initializeSDK(program.opts());
            
            let sessionId;
            
            if (options.interactive || !options.session) {
                const answer = await inquirer.prompt([
                    {
                        type: 'input',
                        name: 'sessionId',
                        message: 'Session ID:',
                        default: options.session,
                        validate: input => input.trim() !== '' || 'Session ID is required'
                    }
                ]);
                sessionId = answer.sessionId;
            } else {
                sessionId = options.session;
            }
            
            const spinner = ora('Generating summary...').start();
            
            const result = await sdk.summarizeSession(sessionId);
            
            spinner.succeed();
            
            console.log(chalk.green('\n✅ Session Summary Generated:\n'));
            
            if (program.opts().format === 'json') {
                console.log(JSON.stringify(result.data.summary, null, 2));
            } else {
                const summary = result.data.summary;
                console.log(chalk.blue('Session ID:'), summary.sessionId);
                console.log(chalk.blue('User ID:'), summary.userId);
                console.log(chalk.blue('Memory Count:'), summary.memoryCount);
                console.log(chalk.blue('Importance Score:'), summary.importanceScore?.toFixed(2));
                console.log(chalk.blue('\nSummary:'));
                console.log(summary.summaryText);
                console.log(chalk.blue('\nKey Topics:'));
                console.log(summary.keyTopics?.join(', ') || 'None');
            }
            
        } catch (error) {
            handleError(error);
        }
    });

// Session management commands
program
    .command('sessions')
    .description('Manage sessions')
    .option('--user <userId>', 'User ID')
    .option('--list', 'List user sessions')
    .option('--create', 'Create new session')
    .option('--name <name>', 'Session name')
    .option('-i, --interactive', 'Interactive mode')
    .action(async (options) => {
        try {
            sdk = initializeSDK(program.opts());
            
            if (options.create) {
                // Create session
                let sessionData;
                
                if (options.interactive || !options.user) {
                    const answers = await inquirer.prompt([
                        {
                            type: 'input',
                            name: 'userId',
                            message: 'User ID:',
                            default: options.user,
                            validate: input => input.trim() !== '' || 'User ID is required'
                        },
                        {
                            type: 'input',
                            name: 'name',
                            message: 'Session name:',
                            default: options.name
                        }
                    ]);
                    
                    sessionData = answers;
                } else {
                    sessionData = {
                        userId: options.user,
                        name: options.name
                    };
                }
                
                const spinner = ora('Creating session...').start();
                const result = await sdk.createSession(sessionData);
                spinner.succeed();
                
                displaySuccess('Session created successfully', {
                    sessionId: result.data.sessionId,
                    userId: result.data.userId,
                    name: result.data.name
                });
                
            } else {
                // List sessions (default)
                let userId;
                
                if (options.interactive || !options.user) {
                    const answer = await inquirer.prompt([
                        {
                            type: 'input',
                            name: 'userId',
                            message: 'User ID:',
                            default: options.user,
                            validate: input => input.trim() !== '' || 'User ID is required'
                        }
                    ]);
                    userId = answer.userId;
                } else {
                    userId = options.user;
                }
                
                const spinner = ora('Getting user sessions...').start();
                const result = await sdk.getUserSessions(userId);
                spinner.succeed();
                
                if (result.data.sessions.length === 0) {
                    console.log(chalk.yellow('\n📭 No sessions found for this user'));
                    return;
                }
                
                console.log(chalk.green(`\n✅ Found ${result.data.sessions.length} sessions:\n`));
                
                if (program.opts().format === 'json') {
                    console.log(JSON.stringify(result.data.sessions, null, 2));
                } else {
                    const table = new Table({
                        head: ['Session ID', 'Name', 'Memories', 'Last Active'],
                        colWidths: [25, 30, 12, 20]
                    });
                    
                    result.data.sessions.forEach(session => {
                        table.push([
                            session.id.substring(0, 22) + '...',
                            session.name || 'Unnamed',
                            session.memoryCount || 0,
                            new Date(session.lastActive).toLocaleDateString()
                        ]);
                    });
                    
                    console.log(table.toString());
                }
            }
            
        } catch (error) {
            handleError(error);
        }
    });

// Statistics command
program
    .command('stats')
    .description('Get system statistics')
    .option('--user <userId>', 'Get user-specific stats')
    .option('--memory', 'Get memory statistics')
    .option('--health', 'Get health status')
    .action(async (options) => {
        try {
            sdk = initializeSDK(program.opts());
            
            const spinner = ora('Getting statistics...').start();
            
            let result;
            let title;
            
            if (options.user) {
                result = await sdk.getUserStats(options.user);
                title = `User Statistics (${options.user})`;
            } else if (options.memory) {
                result = await sdk.getMemoryStats();
                title = 'Memory Statistics';
            } else if (options.health) {
                result = await sdk.getHealth();
                title = 'Health Status';
            } else {
                result = await sdk.getSystemStats();
                title = 'System Statistics';
            }
            
            spinner.succeed();
            
            console.log(chalk.green(`\n✅ ${title}:\n`));
            
            if (program.opts().format === 'json') {
                console.log(JSON.stringify(result.data, null, 2));
            } else {
                // Pretty print based on type
                if (options.health) {
                    const health = result.data;
                    console.log(chalk.blue('Status:'), health.status === 'healthy' ? chalk.green(health.status) : chalk.red(health.status));
                    console.log(chalk.blue('Uptime:'), `${Math.floor(health.metrics?.uptime || 0)}s`);
                    console.log(chalk.blue('Memory Usage:'), `${health.metrics?.memoryUsagePercent?.toFixed(1) || 0}%`);
                } else {
                    console.log(JSON.stringify(result.data, null, 2));
                }
            }
            
        } catch (error) {
            handleError(error);
        }
    });

// Export command
program
    .command('export')
    .description('Export user memories')
    .option('--user <userId>', 'User ID')
    .option('--output <file>', 'Output file path')
    .option('-i, --interactive', 'Interactive mode')
    .action(async (options) => {
        try {
            sdk = initializeSDK(program.opts());
            
            let userId, outputPath;
            
            if (options.interactive || !options.user) {
                const answers = await inquirer.prompt([
                    {
                        type: 'input',
                        name: 'userId',
                        message: 'User ID:',
                        default: options.user,
                        validate: input => input.trim() !== '' || 'User ID is required'
                    },
                    {
                        type: 'input',
                        name: 'outputPath',
                        message: 'Output file path:',
                        default: options.output || `mindcache-export-${Date.now()}.json`
                    }
                ]);
                
                userId = answers.userId;
                outputPath = answers.outputPath;
            } else {
                userId = options.user;
                outputPath = options.output || `mindcache-export-${userId}-${Date.now()}.json`;
            }
            
            const spinner = ora('Exporting memories...').start();
            
            const result = await sdk.exportMemories(userId);
            
            // Write to file
            fs.writeFileSync(outputPath, JSON.stringify(result.data.memories, null, 2));
            
            spinner.succeed();
            
            displaySuccess('Memories exported successfully', {
                userId,
                outputPath,
                memoryCount: result.data.memories.length
            });
            
        } catch (error) {
            handleError(error);
        }
    });

// Health check command
program
    .command('ping')
    .description('Check API connectivity')
    .action(async () => {
        try {
            sdk = initializeSDK(program.opts());
            
            const spinner = ora('Checking API connectivity...').start();
            
            const isAvailable = await sdk.isApiAvailable();
            
            if (isAvailable) {
                spinner.succeed();
                console.log(chalk.green('\n✅ API is available and responding'));
                
                // Get basic info
                const info = await sdk.getApiInfo();
                console.log(chalk.blue('\nAPI Info:'));
                console.log(`Name: ${info.name || 'MindCache API'}`);
                console.log(`Version: ${info.version || 'Unknown'}`);
                console.log(`URL: ${sdk.baseUrl}`);
            } else {
                spinner.fail();
                console.log(chalk.red('\n❌ API is not available'));
                console.log(`URL: ${sdk.baseUrl}`);
                process.exit(1);
            }
            
        } catch (error) {
            handleError(error);
        }
    });

// Interactive mode command
program
    .command('interactive')
    .alias('i')
    .description('Start interactive mode')
    .action(async () => {
        try {
            sdk = initializeSDK(program.opts());
            
            console.log(chalk.blue('\n🧠 MindCache Interactive CLI\n'));
            console.log(chalk.gray(`Connected to: ${sdk.baseUrl}\n`));
            
            // Check API availability
            const spinner = ora('Checking API connectivity...').start();
            const isAvailable = await sdk.isApiAvailable();
            
            if (!isAvailable) {
                spinner.fail();
                console.log(chalk.red('❌ API is not available. Please check your connection and try again.'));
                process.exit(1);
            }
            
            spinner.succeed('Connected to API');
            
            while (true) {
                const { action } = await inquirer.prompt([
                    {
                        type: 'list',
                        name: 'action',
                        message: 'What would you like to do?',
                        choices: [
                            { name: '💾 Save Memory', value: 'save' },
                            { name: '🔍 Recall Memories', value: 'recall' },
                            { name: '📋 Summarize Session', value: 'summarize' },
                            { name: '📁 Manage Sessions', value: 'sessions' },
                            { name: '📊 View Statistics', value: 'stats' },
                            { name: '📤 Export Memories', value: 'export' },
                            { name: '🏥 Health Check', value: 'health' },
                            { name: '❌ Exit', value: 'exit' }
                        ]
                    }
                ]);
                
                if (action === 'exit') {
                    console.log(chalk.blue('\n👋 Goodbye!'));
                    break;
                }
                
                try {
                    switch (action) {
                        case 'save':
                            await program.commands.find(cmd => cmd.name() === 'save').action({ interactive: true });
                            break;
                        case 'recall':
                            await program.commands.find(cmd => cmd.name() === 'recall').action({ interactive: true });
                            break;
                        case 'summarize':
                            await program.commands.find(cmd => cmd.name() === 'summarize').action({ interactive: true });
                            break;
                        case 'sessions':
                            const { sessionAction } = await inquirer.prompt([
                                {
                                    type: 'list',
                                    name: 'sessionAction',
                                    message: 'Session action:',
                                    choices: [
                                        { name: 'List Sessions', value: 'list' },
                                        { name: 'Create Session', value: 'create' }
                                    ]
                                }
                            ]);
                            
                            await program.commands.find(cmd => cmd.name() === 'sessions').action({ 
                                interactive: true, 
                                [sessionAction]: true 
                            });
                            break;
                        case 'stats':
                            const { statsType } = await inquirer.prompt([
                                {
                                    type: 'list',
                                    name: 'statsType',
                                    message: 'Statistics type:',
                                    choices: [
                                        { name: 'System Stats', value: 'system' },
                                        { name: 'User Stats', value: 'user' },
                                        { name: 'Memory Stats', value: 'memory' }
                                    ]
                                }
                            ]);
                            
                            const statsOptions = { interactive: true };
                            if (statsType === 'user') statsOptions.user = '';
                            if (statsType === 'memory') statsOptions.memory = true;
                            
                            await program.commands.find(cmd => cmd.name() === 'stats').action(statsOptions);
                            break;
                        case 'export':
                            await program.commands.find(cmd => cmd.name() === 'export').action({ interactive: true });
                            break;
                        case 'health':
                            await program.commands.find(cmd => cmd.name() === 'stats').action({ health: true });
                            break;
                    }
                } catch (error) {
                    console.log(chalk.red(`\n❌ Error: ${error.message}`));
                }
                
                console.log('\n' + chalk.gray('─'.repeat(50)) + '\n');
            }
            
        } catch (error) {
            handleError(error);
        }
    });

// Parse command line arguments
program.parse();

// If no command specified, show help
if (!process.argv.slice(2).length) {
    program.outputHelp();
}