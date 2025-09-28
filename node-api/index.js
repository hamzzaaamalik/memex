const express = require('express')
const cors = require('cors')
const helmet = require('helmet')
const rateLimit = require('express-rate-limit')
const compression = require('compression')
const morgan = require('morgan')
const path = require('path')
const fs = require('fs')

// Import our route handlers
const memoryRoutes = require('./routes/memoryRoutes')
const sessionRoutes = require('./routes/sessionRoutes')
const statsRoutes = require('./routes/statsRoutes')

// Import middleware
const errorHandler = require('./middleware/errorHandler')
const validateRequest = require('./middleware/validateRequest')

// Import Rust bridge
const RustBridge = require('./dbBridge')

const app = express()
const PORT = process.env.PORT || 3000
const NODE_ENV = process.env.NODE_ENV || 'development'

// Initialize Rust bridge
let rustBridge

/**
 * Initialize the MindCache server
 */
async function initializeServer () {
  try {
    console.log('🧠 Initializing MindCache Node.js API Server...')

    // Initialize Rust bridge
    const config = {
      storage_path: process.env.MINDCACHE_STORAGE_PATH || './mindcache_data',
      auto_decay_enabled: process.env.AUTO_DECAY_ENABLED !== 'false',
      decay_interval_hours: parseInt(process.env.DECAY_INTERVAL_HOURS) || 24,
      default_memory_ttl_hours: parseInt(process.env.DEFAULT_TTL_HOURS) || 720, // 30 days
      enable_compression: process.env.ENABLE_COMPRESSION !== 'false',
      max_memories_per_user: parseInt(process.env.MAX_MEMORIES_PER_USER) || 10000,
      importance_threshold: parseFloat(process.env.IMPORTANCE_THRESHOLD) || 0.3
    }

    console.log('⚙️ Configuration:', {
      storage_path: config.storage_path,
      auto_decay_enabled: config.auto_decay_enabled,
      max_memories_per_user: config.max_memories_per_user,
      importance_threshold: config.importance_threshold
    })

    rustBridge = new RustBridge(config)
    await rustBridge.initialize()

    console.log('✅ Rust bridge initialized successfully')

    // Make bridge available to routes
    app.locals.rustBridge = rustBridge

    return true
  } catch (error) {
    console.error('❌ Failed to initialize MindCache server:', error)
    process.exit(1)
  }
}

/**
 * Configure Express middleware
 */
function configureMiddleware () {
  console.log('🔧 Configuring middleware...')

  // Security middleware
  app.use(helmet({
    contentSecurityPolicy: {
      directives: {
        defaultSrc: ["'self'"],
        scriptSrc: ["'self'", "'unsafe-inline'"],
        styleSrc: ["'self'", "'unsafe-inline'"],
        imgSrc: ["'self'", 'data:', 'https:']
      }
    },
    crossOriginEmbedderPolicy: false
  }))

  // CORS configuration
  const corsOptions = {
    origin: function (origin, callback) {
      // Allow requests with no origin (mobile apps, Postman, etc.)
      if (!origin) return callback(null, true)

      // In development, allow all origins
      if (NODE_ENV === 'development') {
        return callback(null, true)
      }

      // In production, check against whitelist
      const allowedOrigins = (process.env.ALLOWED_ORIGINS || '').split(',')
      if (allowedOrigins.includes(origin)) {
        return callback(null, true)
      } else {
        return callback(new Error('Not allowed by CORS'))
      }
    },
    methods: ['GET', 'POST', 'PUT', 'DELETE'],
    allowedHeaders: ['Content-Type', 'Authorization', 'X-Requested-With'],
    credentials: true
  }

  app.use(cors(corsOptions))

  // Rate limiting
  const limiter = rateLimit({
    windowMs: 15 * 60 * 1000, // 15 minutes
    max: process.env.RATE_LIMIT_MAX || 1000, // Limit each IP to 1000 requests per windowMs
    message: {
      error: 'Too many requests from this IP, please try again later.',
      code: 'RATE_LIMIT_EXCEEDED'
    },
    standardHeaders: true,
    legacyHeaders: false
  })
  app.use('/api/', limiter)

  // Compression middleware
  app.use(compression())

  // Logging middleware
  if (NODE_ENV === 'development') {
    app.use(morgan('dev'))
  } else {
    app.use(morgan('combined'))
  }

  // Body parsing middleware
  app.use(express.json({
    limit: '10mb',
    strict: true
  }))
  app.use(express.urlencoded({
    extended: true,
    limit: '10mb'
  }))

  // Request validation middleware
  // app.use('/api/', validateRequest);

  console.log('✅ Middleware configured')
}

/**
 * Configure routes
 */
function configureRoutes () {
  console.log('🛣️ Configuring routes...')

  // Health check endpoint
  app.get('/health', (req, res) => {
    res.json({
      status: 'healthy',
      timestamp: new Date().toISOString(),
      version: process.env.npm_package_version || '1.0.0',
      environment: NODE_ENV,
      uptime: process.uptime()
    })
  })

  // API documentation endpoint
  app.get('/api', (req, res) => {
    res.json({
      name: 'MindCache API',
      version: '1.0.0',
      description: 'A lightweight, local-first memory engine for AI applications',
      endpoints: {
        memory: {
          'POST /api/memory/save': 'Save a new memory',
          'POST /api/memory/recall': 'Recall memories with filters',
          'POST /api/memory/summarize': 'Generate session summary',
          'DELETE /api/memory/decay': 'Run memory decay process'
        },
        sessions: {
          'POST /api/sessions': 'Create a new session',
          'GET /api/sessions/:userId': 'Get user sessions',
          'GET /api/sessions/:userId/:sessionId': 'Get specific session',
          'POST /api/sessions/search': 'Search sessions by content'
        },
        stats: {
          'GET /api/stats': 'Get system statistics',
          'GET /api/stats/user/:userId': 'Get user-specific stats'
        }
      },
      documentation: '/api/docs'
    })
  })

  // API Routes
  app.use('/api/memory', memoryRoutes)
  app.use('/api/sessions', sessionRoutes)
  app.use('/api/stats', statsRoutes)

  // Serve static documentation (if available)
  const docsPath = path.join(__dirname, '../docs')
  if (fs.existsSync(docsPath)) {
    app.use('/api/docs', express.static(docsPath))
  }

  // 404 handler for API routes
  app.use('/api/*', (req, res) => {
    res.status(404).json({
      error: 'API endpoint not found',
      code: 'ENDPOINT_NOT_FOUND',
      path: req.path,
      method: req.method,
      timestamp: new Date().toISOString()
    })
  })

  // Root route
  app.get('/', (req, res) => {
    res.json({
      message: 'Welcome to MindCache API',
      version: '1.0.0',
      documentation: '/api',
      health: '/health'
    })
  })

  console.log('✅ Routes configured')
}

/**
 * Configure error handling
 */
function configureErrorHandling () {
  console.log('🛡️ Configuring error handling...')

  // Global error handler
  app.use(errorHandler)

  // Graceful shutdown handlers
  process.on('SIGTERM', gracefulShutdown)
  process.on('SIGINT', gracefulShutdown)
  process.on('uncaughtException', (error) => {
    console.error('Uncaught Exception:', error)
    gracefulShutdown()
  })
  process.on('unhandledRejection', (reason, promise) => {
    console.error('Unhandled Rejection at:', promise, 'reason:', reason)
    gracefulShutdown()
  })

  console.log('✅ Error handling configured')
}

/**
 * Graceful shutdown handler
 */
async function gracefulShutdown () {
  console.log('🛑 Received shutdown signal, shutting down gracefully...')

  try {
    // Close Rust bridge
    if (rustBridge) {
      await rustBridge.cleanup()
      console.log('✅ Rust bridge cleaned up')
    }

    // Close server
    if (server) {
      server.close(() => {
        console.log('✅ HTTP server closed')
        process.exit(0)
      })

      // Force close after 30 seconds
      setTimeout(() => {
        console.error('❌ Could not close connections in time, forcefully shutting down')
        process.exit(1)
      }, 30000)
    } else {
      process.exit(0)
    }
  } catch (error) {
    console.error('❌ Error during shutdown:', error)
    process.exit(1)
  }
}

/**
 * Start the server
 */
async function startServer () {
  try {
    // Initialize components
    await initializeServer()
    configureMiddleware()
    configureRoutes()
    // configureErrorHandling();

    // Start HTTP server
    const server = app.listen(PORT, () => {
      console.log(`🚀 MindCache API Server running on port ${PORT}`)
      console.log(`📊 Environment: ${NODE_ENV}`)
      console.log(`🌐 Health check: http://localhost:${PORT}/health`)
      console.log(`📖 API docs: http://localhost:${PORT}/api`)
      console.log('🧠 MindCache ready for connections!')
    })

    // Store server reference for graceful shutdown
    global.server = server

    return server
  } catch (error) {
    console.error('❌ Failed to start server:', error)
    process.exit(1)
  }
}

// Start server if this file is run directly
if (require.main === module) {
  startServer().catch(error => {
    console.error('❌ Server startup failed:', error)
    process.exit(1)
  })
}

module.exports = { app, startServer, gracefulShutdown }
