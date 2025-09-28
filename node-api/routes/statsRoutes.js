const express = require('express')
const router = express.Router()
const statsController = require('../controllers/statsController')

/**
 * @route GET /api/stats
 * @desc Get overall system statistics
 * @access Public
 */
router.get('/', statsController.getSystemStats)

/**
 * @route GET /api/stats/user/:userId
 * @desc Get statistics for a specific user
 * @access Public
 * @params userId: string
 */
router.get('/user/:userId', statsController.getUserStats)

/**
 * @route GET /api/stats/memory
 * @desc Get memory usage and performance statistics
 * @access Public
 */
router.get('/memory', statsController.getMemoryStats)

/**
 * @route GET /api/stats/sessions
 * @desc Get session-related statistics
 * @access Public
 */
router.get('/sessions', statsController.getSessionStats)

/**
 * @route GET /api/stats/decay
 * @desc Get memory decay statistics
 * @access Public
 */
router.get('/decay', statsController.getDecayStats)

/**
 * @route GET /api/stats/performance
 * @desc Get API performance metrics
 * @access Public
 */
router.get('/performance', statsController.getPerformanceStats)

/**
 * @route GET /api/stats/health
 * @desc Get detailed health information
 * @access Public
 */
router.get('/health', statsController.getHealthStats)

module.exports = router
