const express = require('express')
const router = express.Router()
const sessionController = require('../controllers/sessionController')
const { validateSessionCreate, validateSessionSearch } = require('../middleware/validation')

/**
 * @route POST /api/sessions
 * @desc Create a new session
 * @access Public
 * @body {
 *   userId: string,
 *   name?: string,
 *   metadata?: object
 * }
 */
router.post('/', validateSessionCreate, sessionController.createSession)

/**
 * @route GET /api/sessions/:userId
 * @desc Get all sessions for a user
 * @access Public
 * @params userId: string
 */
router.get('/:userId', sessionController.getUserSessions)

/**
 * @route GET /api/sessions/:userId/:sessionId
 * @desc Get a specific session
 * @access Public
 * @params userId: string, sessionId: string
 */
router.get('/:userId/:sessionId', sessionController.getSession)

/**
 * @route GET /api/sessions/:userId/:sessionId/memories
 * @desc Get all memories for a specific session
 * @access Public
 * @params userId: string, sessionId: string
 */
router.get('/:userId/:sessionId/memories', sessionController.getSessionMemories)

/**
 * @route POST /api/sessions/search
 * @desc Search sessions by content keywords
 * @access Public
 * @body {
 *   userId: string,
 *   keywords: string[]
 * }
 */
router.post('/search', validateSessionSearch, sessionController.searchSessions)

/**
 * @route PUT /api/sessions/:userId/:sessionId
 * @desc Update session metadata
 * @access Public
 * @params userId: string, sessionId: string
 * @body {
 *   name?: string,
 *   tags?: string[],
 *   metadata?: object
 * }
 */
router.put('/:userId/:sessionId', sessionController.updateSession)

/**
 * @route DELETE /api/sessions/:userId/:sessionId
 * @desc Delete a session and all its memories
 * @access Public
 * @params userId: string, sessionId: string
 */
router.delete('/:userId/:sessionId', sessionController.deleteSession)

/**
 * @route POST /api/sessions/:sessionId/summarize
 * @desc Generate summary for a session (alternative endpoint)
 * @access Public
 * @params sessionId: string
 */
router.post('/:sessionId/summarize', sessionController.summarizeSession)

module.exports = router
