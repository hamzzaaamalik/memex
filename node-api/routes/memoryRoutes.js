const express = require('express')
const router = express.Router()
const memoryController = require('../controllers/memoryController')
const { validateMemorySave, validateMemoryRecall, validateMemorySummarize } = require('../middleware/validation')

/**
 * @route POST /api/memory/save
 * @desc Save a new memory item
 * @access Public
 * @body {
 *   userId: string,
 *   sessionId: string,
 *   content: string,
 *   metadata?: object,
 *   importance?: number (0-1),
 *   ttlHours?: number
 * }
 */
router.post('/save', validateMemorySave, memoryController.saveMemory)

/**
 * @route POST /api/memory/recall
 * @desc Recall memories with filters
 * @access Public
 * @body {
 *   userId: string,
 *   query?: string,
 *   sessionId?: string,
 *   dateFrom?: string (ISO date),
 *   dateTo?: string (ISO date),
 *   limit?: number,
 *   minImportance?: number (0-1),
 *   keywords?: string[]
 * }
 */
router.post('/recall', validateMemoryRecall, memoryController.recallMemories)

/**
 * @route POST /api/memory/summarize
 * @desc Generate a summary for a session
 * @access Public
 * @body {
 *   sessionId: string
 * }
 */
router.post('/summarize', validateMemorySummarize, memoryController.summarizeSession)

/**
 * @route GET /api/memory/export/:userId
 * @desc Export all memories for a user
 * @access Public
 * @params userId: string
 */
router.get('/export/:userId', memoryController.exportUserMemories)

/**
 * @route DELETE /api/memory/decay
 * @desc Run memory decay process
 * @access Public
 * @body {
 *   force?: boolean
 * }
 */
router.delete('/decay', memoryController.runDecayProcess)

/**
 * @route GET /api/memory/search
 * @desc Advanced memory search with query parameters
 * @access Public
 * @query {
 *   userId: string,
 *   q?: string,
 *   session?: string,
 *   from?: string (ISO date),
 *   to?: string (ISO date),
 *   limit?: number,
 *   importance?: number,
 *   tags?: string (comma-separated)
 * }
 */
router.get('/search', memoryController.searchMemories)

/**
 * @route POST /api/memory/bulk
 * @desc Save multiple memories in a batch
 * @access Public
 * @body {
 *   memories: [{
 *     userId: string,
 *     sessionId: string,
 *     content: string,
 *     metadata?: object,
 *     importance?: number,
 *     ttlHours?: number
 *   }]
 * }
 */
router.post('/bulk', memoryController.bulkSaveMemories)

/**
 * @route GET /api/memory/stats/:userId
 * @desc Get memory statistics for a specific user
 * @access Public
 * @params userId: string
 */
router.get('/stats/:userId', memoryController.getUserMemoryStats)

module.exports = router
