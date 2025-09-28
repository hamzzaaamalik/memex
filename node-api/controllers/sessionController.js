const { asyncHandler, createError } = require('../middleware/errorHandler')

/**
 * Session Controller - Handles all session-related operations
 */

/**
 * Create a new session
 * @route POST /api/sessions
 */
const createSession = asyncHandler(async (req, res) => {
  const { userId, name, metadata } = req.body
  const rustBridge = req.app.locals.rustBridge

  try {
    // Create session using Rust bridge
    const sessionId = await rustBridge.createSession(userId, name, metadata)

    res.status(201).json({
      success: true,
      data: {
        sessionId,
        userId,
        name: name || null,
        metadata: metadata || {},
        createdAt: new Date().toISOString()
      },
      message: 'Session created successfully'
    })
  } catch (error) {
    console.error('Error creating session:', error)
    throw createError(
      error.message || 'Failed to create session',
      'SESSION_CREATE_ERROR',
      500,
      { userId, name }
    )
  }
})

/**
 * Get all sessions for a user
 * @route GET /api/sessions/:userId
 */
const getUserSessions = asyncHandler(async (req, res) => {
  const { userId } = req.params
  const rustBridge = req.app.locals.rustBridge

  try {
    // Get user sessions using Rust bridge
    const sessions = await rustBridge.getUserSessions(userId)

    res.json({
      success: true,
      data: {
        sessions,
        count: sessions.length,
        userId
      },
      message: `Found ${sessions.length} sessions for user`
    })
  } catch (error) {
    console.error('Error getting user sessions:', error)
    throw createError(
      error.message || 'Failed to get user sessions',
      'SESSION_LIST_ERROR',
      500,
      { userId }
    )
  }
})

/**
 * Get a specific session
 * @route GET /api/sessions/:userId/:sessionId
 */
const getSession = asyncHandler(async (req, res) => {
  const { userId, sessionId } = req.params
  const rustBridge = req.app.locals.rustBridge

  try {
    // Get session details using Rust bridge
    const session = await rustBridge.getSession(sessionId)

    if (!session) {
      throw createError(
        'Session not found',
        'SESSION_NOT_FOUND',
        404,
        { userId, sessionId }
      )
    }

    // Verify the session belongs to the user
    if (session.userId !== userId) {
      throw createError(
        'Session does not belong to this user',
        'SESSION_ACCESS_DENIED',
        403,
        { userId, sessionId }
      )
    }

    res.json({
      success: true,
      data: {
        session
      },
      message: 'Session retrieved successfully'
    })
  } catch (error) {
    console.error('Error getting session:', error)

    if (error.code === 'SESSION_NOT_FOUND' || error.code === 'SESSION_ACCESS_DENIED') {
      throw error
    }

    throw createError(
      error.message || 'Failed to get session',
      'SESSION_GET_ERROR',
      500,
      { userId, sessionId }
    )
  }
})

/**
 * Get all memories for a specific session
 * @route GET /api/sessions/:userId/:sessionId/memories
 */
const getSessionMemories = asyncHandler(async (req, res) => {
  const { userId, sessionId } = req.params
  const { limit, offset } = req.query
  const rustBridge = req.app.locals.rustBridge

  try {
    // Get session memories using Rust bridge
    const memories = await rustBridge.getSessionMemories(userId, sessionId)

    // Apply pagination if requested
    let paginatedMemories = memories
    const limitNum = parseInt(limit)
    const offsetNum = parseInt(offset) || 0

    if (limitNum && limitNum > 0) {
      const startIndex = offsetNum
      const endIndex = startIndex + limitNum
      paginatedMemories = memories.slice(startIndex, endIndex)
    }

    res.json({
      success: true,
      data: {
        memories: paginatedMemories,
        pagination: {
          total: memories.length,
          returned: paginatedMemories.length,
          offset: offsetNum,
          limit: limitNum || null
        },
        sessionId,
        userId
      },
      message: `Retrieved ${paginatedMemories.length} memories from session`
    })
  } catch (error) {
    console.error('Error getting session memories:', error)
    throw createError(
      error.message || 'Failed to get session memories',
      'SESSION_MEMORIES_ERROR',
      500,
      { userId, sessionId }
    )
  }
})

/**
 * Search sessions by content keywords
 * @route POST /api/sessions/search
 */
const searchSessions = asyncHandler(async (req, res) => {
  const { userId, keywords } = req.body
  const rustBridge = req.app.locals.rustBridge

  try {
    // Search sessions using Rust bridge
    const sessions = await rustBridge.searchSessions(userId, keywords)

    res.json({
      success: true,
      data: {
        sessions,
        count: sessions.length,
        searchCriteria: {
          userId,
          keywords
        }
      },
      message: `Found ${sessions.length} sessions matching search criteria`
    })
  } catch (error) {
    console.error('Error searching sessions:', error)
    throw createError(
      error.message || 'Failed to search sessions',
      'SESSION_SEARCH_ERROR',
      500,
      { userId, keywords }
    )
  }
})

/**
 * Update session metadata
 * @route PUT /api/sessions/:userId/:sessionId
 */
const updateSession = asyncHandler(async (req, res) => {
  const { userId, sessionId } = req.params
  const { name, tags, metadata } = req.body
  const rustBridge = req.app.locals.rustBridge

  try {
    // First verify the session exists and belongs to the user
    const existingSession = await rustBridge.getSession(sessionId)

    if (!existingSession) {
      throw createError(
        'Session not found',
        'SESSION_NOT_FOUND',
        404,
        { userId, sessionId }
      )
    }

    if (existingSession.userId !== userId) {
      throw createError(
        'Session does not belong to this user',
        'SESSION_ACCESS_DENIED',
        403,
        { userId, sessionId }
      )
    }

    // Update session using Rust bridge
    const updatedSession = await rustBridge.updateSession(sessionId, {
      name,
      tags,
      metadata
    })

    res.json({
      success: true,
      data: {
        session: updatedSession,
        updatedAt: new Date().toISOString()
      },
      message: 'Session updated successfully'
    })
  } catch (error) {
    console.error('Error updating session:', error)

    if (error.code === 'SESSION_NOT_FOUND' || error.code === 'SESSION_ACCESS_DENIED') {
      throw error
    }

    throw createError(
      error.message || 'Failed to update session',
      'SESSION_UPDATE_ERROR',
      500,
      { userId, sessionId }
    )
  }
})

/**
 * Delete a session and all its memories
 * @route DELETE /api/sessions/:userId/:sessionId
 */
const deleteSession = asyncHandler(async (req, res) => {
  const { userId, sessionId } = req.params
  const { confirm } = req.query
  const rustBridge = req.app.locals.rustBridge

  try {
    // Require confirmation for destructive operation
    if (confirm !== 'true') {
      throw createError(
        'Session deletion requires confirmation. Add ?confirm=true to the request',
        'CONFIRMATION_REQUIRED',
        400,
        { userId, sessionId }
      )
    }

    // First verify the session exists and belongs to the user
    const existingSession = await rustBridge.getSession(sessionId)

    if (!existingSession) {
      throw createError(
        'Session not found',
        'SESSION_NOT_FOUND',
        404,
        { userId, sessionId }
      )
    }

    if (existingSession.userId !== userId) {
      throw createError(
        'Session does not belong to this user',
        'SESSION_ACCESS_DENIED',
        403,
        { userId, sessionId }
      )
    }

    // Delete session using Rust bridge
    const deletionResult = await rustBridge.deleteSession(sessionId)

    res.json({
      success: true,
      data: {
        sessionId,
        userId,
        memoriesDeleted: deletionResult.memoriesDeleted || 0,
        deletedAt: new Date().toISOString()
      },
      message: `Session and ${deletionResult.memoriesDeleted || 0} memories deleted successfully`
    })
  } catch (error) {
    console.error('Error deleting session:', error)

    if (error.code === 'SESSION_NOT_FOUND' ||
            error.code === 'SESSION_ACCESS_DENIED' ||
            error.code === 'CONFIRMATION_REQUIRED') {
      throw error
    }

    throw createError(
      error.message || 'Failed to delete session',
      'SESSION_DELETE_ERROR',
      500,
      { userId, sessionId }
    )
  }
})

/**
 * Generate summary for a session (alternative endpoint)
 * @route POST /api/sessions/:sessionId/summarize
 */
const summarizeSession = asyncHandler(async (req, res) => {
  const { sessionId } = req.params
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
    console.error('Error generating session summary:', error)

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

module.exports = {
  createSession,
  getUserSessions,
  getSession,
  getSessionMemories,
  searchSessions,
  updateSession,
  deleteSession,
  summarizeSession
}
