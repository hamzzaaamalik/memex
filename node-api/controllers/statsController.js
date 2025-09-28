const { asyncHandler, createError } = require('../middleware/errorHandler')
const os = require('os')

/**
 * Statistics Controller - Handles all statistics and monitoring operations
 */

/**
 * Get overall system statistics
 * @route GET /api/stats
 */
const getSystemStats = asyncHandler(async (req, res) => {
  const rustBridge = req.app.locals.rustBridge

  try {
    // Get stats from Rust bridge
    const rustStats = await rustBridge.getStats()

    // Get Node.js system stats
    const systemStats = getNodeSystemStats()

    // Combine all statistics
    const stats = {
      system: systemStats,
      mindcache: rustStats,
      api: {
        uptime: process.uptime(),
        environment: process.env.NODE_ENV || 'development',
        nodeVersion: process.version,
        timestamp: new Date().toISOString()
      }
    }

    res.json({
      success: true,
      data: stats,
      message: 'System statistics retrieved successfully'
    })
  } catch (error) {
    console.error('Error getting system stats:', error)
    throw createError(
      error.message || 'Failed to get system statistics',
      'SYSTEM_STATS_ERROR',
      500
    )
  }
})

/**
 * Get statistics for a specific user
 * @route GET /api/stats/user/:userId
 */
const getUserStats = asyncHandler(async (req, res) => {
  const { userId } = req.params
  const rustBridge = req.app.locals.rustBridge

  try {
    // Get user-specific memories
    const userMemories = await rustBridge.recallMemories({
      userId,
      limit: 10000 // Large limit to get all memories
    })

    // Get user sessions
    const userSessions = await rustBridge.getUserSessions(userId)

    // Calculate user statistics
    const userStats = calculateDetailedUserStats(userMemories, userSessions)

    res.json({
      success: true,
      data: {
        userId,
        stats: userStats,
        calculatedAt: new Date().toISOString()
      },
      message: 'User statistics retrieved successfully'
    })
  } catch (error) {
    console.error('Error getting user stats:', error)
    throw createError(
      error.message || 'Failed to get user statistics',
      'USER_STATS_ERROR',
      500,
      { userId }
    )
  }
})

/**
 * Get memory usage and performance statistics
 * @route GET /api/stats/memory
 */
const getMemoryStats = asyncHandler(async (req, res) => {
  const rustBridge = req.app.locals.rustBridge

  try {
    // Get Rust memory stats
    const rustStats = await rustBridge.getStats()

    // Get Node.js memory usage
    const nodeMemory = process.memoryUsage()

    // Get system memory info
    const systemMemory = {
      total: os.totalmem(),
      free: os.freemem(),
      used: os.totalmem() - os.freemem()
    }

    const memoryStats = {
      node: {
        rss: formatBytes(nodeMemory.rss),
        heapTotal: formatBytes(nodeMemory.heapTotal),
        heapUsed: formatBytes(nodeMemory.heapUsed),
        external: formatBytes(nodeMemory.external),
        arrayBuffers: formatBytes(nodeMemory.arrayBuffers)
      },
      system: {
        total: formatBytes(systemMemory.total),
        free: formatBytes(systemMemory.free),
        used: formatBytes(systemMemory.used),
        usage: Math.round((systemMemory.used / systemMemory.total) * 100)
      },
      rust: rustStats.storage || {}
    }

    res.json({
      success: true,
      data: memoryStats,
      message: 'Memory statistics retrieved successfully'
    })
  } catch (error) {
    console.error('Error getting memory stats:', error)
    throw createError(
      error.message || 'Failed to get memory statistics',
      'MEMORY_STATS_ERROR',
      500
    )
  }
})

/**
 * Get session-related statistics
 * @route GET /api/stats/sessions
 */
const getSessionStats = asyncHandler(async (req, res) => {
  const rustBridge = req.app.locals.rustBridge

  try {
    // Get overall stats
    const rustStats = await rustBridge.getStats()

    // Extract session-related information
    const sessionStats = {
      total: rustStats.sessions?.total_sessions || 0,
      byUser: rustStats.sessions || {},
      timestamp: new Date().toISOString()
    }

    res.json({
      success: true,
      data: sessionStats,
      message: 'Session statistics retrieved successfully'
    })
  } catch (error) {
    console.error('Error getting session stats:', error)
    throw createError(
      error.message || 'Failed to get session statistics',
      'SESSION_STATS_ERROR',
      500
    )
  }
})

/**
 * Get memory decay statistics
 * @route GET /api/stats/decay
 */
const getDecayStats = asyncHandler(async (req, res) => {
  const rustBridge = req.app.locals.rustBridge

  try {
    // Get decay stats from Rust
    const rustStats = await rustBridge.getStats()
    const decayStats = rustStats.decay || {}

    res.json({
      success: true,
      data: {
        decay: decayStats,
        retrievedAt: new Date().toISOString()
      },
      message: 'Decay statistics retrieved successfully'
    })
  } catch (error) {
    console.error('Error getting decay stats:', error)
    throw createError(
      error.message || 'Failed to get decay statistics',
      'DECAY_STATS_ERROR',
      500
    )
  }
})

/**
 * Get API performance metrics
 * @route GET /api/stats/performance
 */
const getPerformanceStats = asyncHandler(async (req, res) => {
  try {
    // Get Node.js performance metrics
    const performanceStats = {
      uptime: process.uptime(),
      cpuUsage: process.cpuUsage(),
      eventLoopDelay: getEventLoopDelay(),
      gc: getGCStats(),
      loadAverage: os.loadavg(),
      cpuCount: os.cpus().length,
      platform: os.platform(),
      arch: os.arch(),
      nodeVersion: process.version,
      v8Version: process.versions.v8
    }

    res.json({
      success: true,
      data: performanceStats,
      message: 'Performance statistics retrieved successfully'
    })
  } catch (error) {
    console.error('Error getting performance stats:', error)
    throw createError(
      error.message || 'Failed to get performance statistics',
      'PERFORMANCE_STATS_ERROR',
      500
    )
  }
})

/**
* Get detailed health information
* @route GET /api/stats/health
*/
const getHealthStats = asyncHandler(async (req, res) => {
  const rustBridge = req.app.locals.rustBridge

  try {
    // Check Rust bridge health
    let rustHealth = 'healthy'
    let rustError = null

    try {
      await rustBridge.getStats()
    } catch (error) {
      rustHealth = 'unhealthy'
      rustError = error.message
    }

    // Get system health metrics
    const memUsage = process.memoryUsage()
    const freeMem = os.freemem()
    const totalMem = os.totalmem()
    const memoryUsagePercent = ((totalMem - freeMem) / totalMem) * 100

    // Determine overall health
    const healthChecks = {
      rust_bridge: rustHealth === 'healthy',
      memory_usage: memoryUsagePercent < 90, // Alert if memory usage > 90%
      heap_usage: (memUsage.heapUsed / memUsage.heapTotal) < 0.9,
      uptime: process.uptime() > 0
    }

    const isHealthy = Object.values(healthChecks).every(check => check)

    const healthStats = {
      status: isHealthy ? 'healthy' : 'degraded',
      checks: healthChecks,
      metrics: {
        uptime: process.uptime(),
        memoryUsagePercent: Math.round(memoryUsagePercent * 100) / 100,
        heapUsagePercent: Math.round((memUsage.heapUsed / memUsage.heapTotal) * 100),
        loadAverage: os.loadavg()[0]
      },
      rust: {
        status: rustHealth,
        error: rustError
      },
      timestamp: new Date().toISOString()
    }

    const statusCode = isHealthy ? 200 : 503

    res.status(statusCode).json({
      success: isHealthy,
      data: healthStats,
      message: isHealthy ? 'System is healthy' : 'System health degraded'
    })
  } catch (error) {
    console.error('Error getting health stats:', error)

    res.status(503).json({
      success: false,
      data: {
        status: 'unhealthy',
        error: error.message,
        timestamp: new Date().toISOString()
      },
      message: 'Health check failed'
    })
  }
})

/**
* Helper Functions
*/

/**
* Get Node.js system statistics
*/
function getNodeSystemStats () {
  const memUsage = process.memoryUsage()

  return {
    platform: os.platform(),
    arch: os.arch(),
    nodeVersion: process.version,
    uptime: process.uptime(),
    pid: process.pid,
    memory: {
      rss: memUsage.rss,
      heapTotal: memUsage.heapTotal,
      heapUsed: memUsage.heapUsed,
      external: memUsage.external
    },
    cpu: {
      loadAverage: os.loadavg(),
      cpuCount: os.cpus().length
    },
    system: {
      totalMemory: os.totalmem(),
      freeMemory: os.freemem(),
      hostname: os.hostname()
    }
  }
}

/**
* Calculate detailed user statistics
*/
function calculateDetailedUserStats (memories, sessions) {
  if (!memories || memories.length === 0) {
    return {
      memories: { total: 0 },
      sessions: { total: 0 },
      activity: { firstActivity: null, lastActivity: null },
      content: { totalCharacters: 0 },
      patterns: {}
    }
  }

  // Memory statistics
  const memoryStats = {
    total: memories.length,
    byImportance: categorizeByImportance(memories),
    averageImportance: calculateAverage(memories.map(m => m.importance || 0)),
    contentLength: {
      total: memories.reduce((sum, m) => sum + (m.content || '').length, 0),
      average: Math.round(memories.reduce((sum, m) => sum + (m.content || '').length, 0) / memories.length)
    }
  }

  // Session statistics
  const sessionStats = {
    total: sessions.length,
    averageMemoriesPerSession: memories.length / (sessions.length || 1),
    mostActiveSession: findMostActiveSession(memories),
    sessionDistribution: calculateSessionDistribution(memories)
  }

  // Activity patterns
  const timestamps = memories.map(m => new Date(m.timestamp))
  const activityStats = {
    firstActivity: new Date(Math.min(...timestamps)).toISOString(),
    lastActivity: new Date(Math.max(...timestamps)).toISOString(),
    timespan: {
      days: Math.ceil((Math.max(...timestamps) - Math.min(...timestamps)) / (1000 * 60 * 60 * 24)),
      hours: Math.ceil((Math.max(...timestamps) - Math.min(...timestamps)) / (1000 * 60 * 60))
    },
    frequency: calculateActivityFrequency(timestamps)
  }

  // Content patterns
  const contentPatterns = analyzeContentPatterns(memories)

  return {
    memories: memoryStats,
    sessions: sessionStats,
    activity: activityStats,
    content: contentPatterns
  }
}

/**
* Categorize memories by importance level
*/
function categorizeByImportance (memories) {
  const categories = { low: 0, medium: 0, high: 0 }

  memories.forEach(memory => {
    const importance = memory.importance || 0
    if (importance < 0.3) categories.low++
    else if (importance < 0.7) categories.medium++
    else categories.high++
  })

  return categories
}

/**
* Calculate average of an array of numbers
*/
function calculateAverage (numbers) {
  if (numbers.length === 0) return 0
  return Math.round((numbers.reduce((sum, num) => sum + num, 0) / numbers.length) * 1000) / 1000
}

/**
* Find the most active session
*/
function findMostActiveSession (memories) {
  const sessionCounts = {}
  memories.forEach(m => {
    sessionCounts[m.sessionId] = (sessionCounts[m.sessionId] || 0) + 1
  })

  const mostActive = Object.entries(sessionCounts)
    .sort(([, a], [, b]) => b - a)[0]

  return mostActive ? { sessionId: mostActive[0], memoryCount: mostActive[1] } : null
}

/**
* Calculate session distribution
*/
function calculateSessionDistribution (memories) {
  const sessionCounts = {}
  memories.forEach(m => {
    sessionCounts[m.sessionId] = (sessionCounts[m.sessionId] || 0) + 1
  })

  const counts = Object.values(sessionCounts)
  return {
    min: Math.min(...counts),
    max: Math.max(...counts),
    average: calculateAverage(counts),
    median: calculateMedian(counts)
  }
}

/**
* Calculate activity frequency
*/
function calculateActivityFrequency (timestamps) {
  const now = new Date()
  const day = 24 * 60 * 60 * 1000
  const week = 7 * day
  const month = 30 * day

  return {
    lastDay: timestamps.filter(t => (now - t) < day).length,
    lastWeek: timestamps.filter(t => (now - t) < week).length,
    lastMonth: timestamps.filter(t => (now - t) < month).length
  }
}

/**
* Analyze content patterns
*/
function analyzeContentPatterns (memories) {
  const allContent = memories.map(m => m.content || '').join(' ')
  const words = allContent.toLowerCase().split(/\s+/).filter(word => word.length > 3)

  // Count word frequency
  const wordCounts = {}
  words.forEach(word => {
    wordCounts[word] = (wordCounts[word] || 0) + 1
  })

  // Get top 10 most frequent words
  const topWords = Object.entries(wordCounts)
    .sort(([, a], [, b]) => b - a)
    .slice(0, 10)
    .map(([word, count]) => ({ word, count }))

  return {
    totalWords: words.length,
    uniqueWords: Object.keys(wordCounts).length,
    averageWordsPerMemory: Math.round(words.length / memories.length),
    topWords
  }
}

/**
* Calculate median of an array
*/
function calculateMedian (numbers) {
  const sorted = [...numbers].sort((a, b) => a - b)
  const mid = Math.floor(sorted.length / 2)
  return sorted.length % 2 === 0
    ? (sorted[mid - 1] + sorted[mid]) / 2
    : sorted[mid]
}

/**
* Format bytes to human readable format
*/
function formatBytes (bytes) {
  if (bytes === 0) return '0 Bytes'
  const k = 1024
  const sizes = ['Bytes', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return {
    value: parseFloat((bytes / Math.pow(k, i)).toFixed(2)),
    unit: sizes[i],
    bytes
  }
}

/**
* Get event loop delay (simplified)
*/
function getEventLoopDelay () {
  const start = process.hrtime.bigint()
  setImmediate(() => {
    const delta = process.hrtime.bigint() - start
    return Number(delta / 1000000n) // Convert to milliseconds
  })
  return 0 // Placeholder - real implementation would use perf_hooks
}

/**
* Get garbage collection stats (simplified)
*/
function getGCStats () {
  // This would require the --expose-gc flag or gc-stats module
  // For now, return placeholder data
  return {
    collections: 0,
    time: 0,
    note: 'GC stats require additional setup'
  }
}

module.exports = {
  getSystemStats,
  getUserStats,
  getMemoryStats,
  getSessionStats,
  getDecayStats,
  getPerformanceStats,
  getHealthStats
}
