const { asyncHandler, createError } = require('../middleware/errorHandler')

/**
 * Memory Controller - Handles all memory-related operations
 */

/**
 * Save a new memory
 * @route POST /api/memory/save
 */
const saveMemory = asyncHandler(async (req, res) => {
  const { userId, sessionId, content, metadata, importance, ttlHours } = req.body
  const rustBridge = req.app.locals.rustBridge

  try {
    // Validate that the session exists or create it implicitly
    if (!sessionId || sessionId.trim() === '') {
      throw createError('Session ID is required', 'MISSING_SESSION_ID', 400)
    }

    // Save the memory using Rust bridge
    const memoryId = await rustBridge.saveMemory({
      userId,
      sessionId,
      content,
      metadata: metadata || {},
      importance: importance || 0.5,
      ttlHours: ttlHours || null
    })

    res.status(201).json({
      success: true,
      data: {
        memoryId,
        userId,
        sessionId,
        timestamp: new Date().toISOString()
      },
      message: 'Memory saved successfully'
    })
  } catch (error) {
    console.error('Error saving memory:', error)
    throw createError(
      error.message || 'Failed to save memory',
      'MEMORY_SAVE_ERROR',
      500,
      { userId, sessionId }
    )
  }
})

/**
 * Recall memories with filters
 * @route POST /api/memory/recall
 */
const recallMemories = asyncHandler(async (req, res) => {
  const {
    userId,
    query,
    sessionId,
    dateFrom,
    dateTo,
    limit,
    minImportance,
    keywords
  } = req.body

  const rustBridge = req.app.locals.rustBridge

  try {
    // Build filter object
    const filter = {
      userId,
      query: query || null,
      sessionId: sessionId || null,
      dateFrom: dateFrom ? new Date(dateFrom) : null,
      dateTo: dateTo ? new Date(dateTo) : null,
      limit: limit || 50,
      minImportance: minImportance || null,
      keywords: keywords || null
    }

    // Recall memories using Rust bridge
    const memories = await rustBridge.recallMemories(filter)

    res.json({
      success: true,
      data: {
        memories,
        filter: {
          userId,
          query,
          sessionId,
          limit: filter.limit
        }
      },
      message: `Found ${memories.data ? memories.data.length : 0} memories`
    })
  } catch (error) {
    console.error('Error recalling memories:', error)
    throw createError(
      error.message || 'Failed to recall memories',
      'MEMORY_RECALL_ERROR',
      500,
      { userId, filter: req.body }
    )
  }
})

/**
 * Generate session summary
 * @route POST /api/memory/summarize
 */
const summarizeSession = asyncHandler(async (req, res) => {
  const { sessionId } = req.body
  const rustBridge = req.app.locals.rustBridge

  try {
    // Generate summary using Rust bridge
    const summary = await rustBridge.summarizeSession(sessionId)

    res.json({
      success: true,
      data: {
        summary,
        sessionId,
        generatedAt: new Date().toISOString()
      },
      message: 'Session summary generated successfully'
    })
  } catch (error) {
    console.error('Error generating summary:', error)

    // Handle specific case where session has no memories
    if (error.message && error.message.includes('No memories found')) {
      throw createError(
        'No memories found for this session',
        'SESSION_EMPTY',
        404,
        { sessionId }
      )
    }

    throw createError(
      error.message || 'Failed to generate session summary',
      'SUMMARY_ERROR',
      500,
      { sessionId }
    )
  }
})

/**
 * Export all memories for a user
 * @route GET /api/memory/export/:userId
 */
const exportUserMemories = asyncHandler(async (req, res) => {
  const { userId } = req.params
  const rustBridge = req.app.locals.rustBridge

  try {
    // Export memories using Rust bridge
    const exportData = await rustBridge.exportUserMemories(userId)

    // Set headers for file download
    res.setHeader('Content-Type', 'application/json')
    res.setHeader('Content-Disposition', `attachment; filename="memex-export-${userId}-${Date.now()}.json"`)

    res.json({
      success: true,
      data: {
        userId,
        exportedAt: new Date().toISOString(),
        memories: JSON.parse(exportData)
      },
      message: 'Memories exported successfully'
    })
  } catch (error) {
    console.error('Error exporting memories:', error)
    throw createError(
      error.message || 'Failed to export memories',
      'EXPORT_ERROR',
      500,
      { userId }
    )
  }
})

/**
 * Run memory decay process
 * @route DELETE /api/memory/decay
 */
const runDecayProcess = asyncHandler(async (req, res) => {
  const { force } = req.body
  const rustBridge = req.app.locals.rustBridge

  try {
    // Run decay process using Rust bridge
    const decayStats = await rustBridge.runDecay(force || false)

    res.json({
      success: true,
      data: {
        decayStats,
        executedAt: new Date().toISOString()
      },
      message: 'Memory decay process completed successfully'
    })
  } catch (error) {
    console.error('Error running decay process:', error)
    throw createError(
      error.message || 'Failed to run decay process',
      'DECAY_ERROR',
      500
    )
  }
})

/**
 * Advanced memory search with query parameters
 * @route GET /api/memory/search
 */
const searchMemories = asyncHandler(async (req, res) => {
  const {
    userId,
    q: query,
    session: sessionId,
    from: dateFrom,
    to: dateTo,
    limit,
    importance: minImportance,
    tags
  } = req.query

  const rustBridge = req.app.locals.rustBridge

  try {
    // Parse tags if provided
    const keywords = tags ? tags.split(',').map(tag => tag.trim()) : null

    // Build filter object
    const filter = {
      userId,
      query: query || null,
      sessionId: sessionId || null,
      dateFrom: dateFrom ? new Date(dateFrom) : null,
      dateTo: dateTo ? new Date(dateTo) : null,
      limit: parseInt(limit) || 50,
      minImportance: parseFloat(minImportance) || null,
      keywords
    }

    // Search memories using Rust bridge
    const memories = await rustBridge.recallMemories(filter)

    res.json({
      success: true,
      data: {
        memories,
        query: {
          userId,
          query,
          sessionId,
          limit: filter.limit,
          tags: keywords
        }
      },
      message: `Found ${memories.data ? memories.data.length : 0} memories matching search criteria`
    })
  } catch (error) {
    console.error('Error searching memories:', error)
    throw createError(
      error.message || 'Failed to search memories',
      'SEARCH_ERROR',
      500,
      { userId, query: req.query }
    )
  }
})

/**
 * Save multiple memories in a batch
 * @route POST /api/memory/bulk
 */
const bulkSaveMemories = asyncHandler(async (req, res) => {
  const { memories } = req.body
  const rustBridge = req.app.locals.rustBridge

  try {
    const results = []
    const errors = []

    // Process each memory individually
    for (let i = 0; i < memories.length; i++) {
      try {
        const memory = memories[i]
        const memoryId = await rustBridge.saveMemory({
          userId: memory.userId,
          sessionId: memory.sessionId,
          content: memory.content,
          metadata: memory.metadata || {},
          importance: memory.importance || 0.5,
          ttlHours: memory.ttlHours || null
        })

        results.push({
          index: i,
          memoryId,
          userId: memory.userId,
          sessionId: memory.sessionId,
          status: 'success'
        })
      } catch (error) {
        console.error(`Error saving memory at index ${i}:`, error)
        errors.push({
          index: i,
          error: error.message,
          memory: memories[i],
          status: 'failed'
        })
      }
    }

    const response = {
      success: errors.length === 0,
      data: {
        results,
        summary: {
          total: memories.length,
          successful: results.length,
          failed: errors.length
        }
      },
      message: `Bulk save completed: ${results.length} successful, ${errors.length} failed`
    }

    // Include errors if any
    if (errors.length > 0) {
      response.data.errors = errors
    }

    // Return 207 Multi-Status if there were partial failures
    const statusCode = errors.length > 0 && results.length > 0 ? 207 : errors.length > 0 ? 400 : 201

    res.status(statusCode).json(response)
  } catch (error) {
    console.error('Error in bulk save operation:', error)
    throw createError(
      error.message || 'Failed to perform bulk save operation',
      'BULK_SAVE_ERROR',
      500,
      { memoryCount: memories.length }
    )
  }
})

/**
 * Get memory statistics for a specific user
 * @route GET /api/memory/stats/:userId
 */
const getUserMemoryStats = asyncHandler(async (req, res) => {
  const { userId } = req.params
  const rustBridge = req.app.locals.rustBridge

  try {
    // Get overall system stats
    const systemStats = await rustBridge.getStats()

    // Get user-specific memories to calculate additional stats
    const userMemoriesResponse = await rustBridge.recallMemories({
      userId,
      limit: 10000 // Large limit to get all memories
    })

    // Extract the actual memories array from the paginated response
    const userMemories = userMemoriesResponse.data || []

    // Calculate user-specific statistics
    const userStats = calculateUserStats(userMemories)

    res.json({
      success: true,
      data: {
        userId,
        stats: {
          ...userStats,
          systemStats: systemStats.storage || {}
        },
        calculatedAt: new Date().toISOString()
      },
      message: 'User memory statistics retrieved successfully'
    })
  } catch (error) {
    console.error('Error getting user memory stats:', error)
    throw createError(
      error.message || 'Failed to get user memory statistics',
      'USER_STATS_ERROR',
      500,
      { userId }
    )
  }
})

/**
 * Calculate user-specific statistics from memories
 */
function calculateUserStats (memories) {
  if (!memories || memories.length === 0) {
    return {
      totalMemories: 0,
      sessionsCount: 0,
      averageImportance: 0,
      oldestMemory: null,
      newestMemory: null,
      contentStats: {
        totalCharacters: 0,
        averageLength: 0
      }
    }
  }

  // Basic counts
  const totalMemories = memories.length
  const uniqueSessions = new Set(memories.map(m => m.sessionId)).size

  // Importance statistics
  const importanceValues = memories.map(m => m.importance || 0)
  const averageImportance = importanceValues.reduce((sum, val) => sum + val, 0) / importanceValues.length

  // Date statistics
  const timestamps = memories.map(m => new Date(m.timestamp))
  const oldestMemory = new Date(Math.min(...timestamps))
  const newestMemory = new Date(Math.max(...timestamps))

  // Content statistics
  const totalCharacters = memories.reduce((sum, m) => sum + (m.content || '').length, 0)
  const averageLength = totalCharacters / totalMemories

  // Session distribution
  const sessionCounts = {}
  memories.forEach(m => {
    sessionCounts[m.sessionId] = (sessionCounts[m.sessionId] || 0) + 1
  })

  const sessionSizes = Object.values(sessionCounts)
  const averageMemoriesPerSession = sessionSizes.reduce((sum, count) => sum + count, 0) / sessionSizes.length

  return {
    totalMemories,
    sessionsCount: uniqueSessions,
    averageImportance: Math.round(averageImportance * 1000) / 1000,
    oldestMemory: oldestMemory.toISOString(),
    newestMemory: newestMemory.toISOString(),
    contentStats: {
      totalCharacters,
      averageLength: Math.round(averageLength)
    },
    sessionStats: {
      averageMemoriesPerSession: Math.round(averageMemoriesPerSession * 100) / 100,
      largestSession: Math.max(...sessionSizes),
      smallestSession: Math.min(...sessionSizes)
    }
  }
}

module.exports = {
  saveMemory,
  recallMemories,
  summarizeSession,
  exportUserMemories,
  runDecayProcess,
  searchMemories,
  bulkSaveMemories,
  getUserMemoryStats
}
