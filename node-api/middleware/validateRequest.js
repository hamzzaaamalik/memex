const rateLimit = require('express-rate-limit')
const { validateContentType } = require('./validation')

/**
 * Request validation middleware
 */
function validateRequest (req, res, next) {
  // Add request ID for tracking
  req.id = generateRequestId()

  // Add timestamp
  req.startTime = Date.now()

  // Log incoming request
  logRequest(req)

  // Validate content type for POST/PUT requests
  validateContentType(req, res, (err) => {
    if (err) return next(err)

    // Validate request size
    validateRequestSize(req, res, (err) => {
      if (err) return next(err)

      // Continue to next middleware
      next()
    })
  })
}

/**
 * Generate unique request ID
 */
function generateRequestId () {
  return Math.random().toString(36).substr(2, 9) + Date.now().toString(36)
}

/**
 * Log incoming requests
 */
function logRequest (req) {
  const logData = {
    id: req.id,
    timestamp: new Date().toISOString(),
    method: req.method,
    url: req.url,
    userAgent: req.get('User-Agent'),
    ip: req.ip,
    contentLength: req.get('Content-Length')
  }

  // Only log in development or if debug is enabled
  if (process.env.NODE_ENV === 'development' || process.env.DEBUG_REQUESTS === 'true') {
    console.log('📨 Request:', JSON.stringify(logData))
  }
}

/**
 * Validate request body size
 */
function validateRequestSize (req, res, next) {
  const maxSize = 10 * 1024 * 1024 // 10MB
  const contentLength = parseInt(req.get('Content-Length')) || 0

  if (contentLength > maxSize) {
    const error = new Error('Request body too large')
    error.code = 'REQUEST_TOO_LARGE'
    error.status = 413
    return next(error)
  }

  next()
}

/**
 * API key validation (if enabled)
 */
function validateApiKey (req, res, next) {
  // Skip if API key validation is disabled
  if (process.env.REQUIRE_API_KEY !== 'true') {
    return next()
  }

  const apiKey = req.get('X-API-Key') || req.query.apiKey
  const validApiKeys = (process.env.VALID_API_KEYS || '').split(',')

  if (!apiKey || !validApiKeys.includes(apiKey)) {
    const error = new Error('Invalid or missing API key')
    error.code = 'INVALID_API_KEY'
    error.status = 401
    return next(error)
  }

  next()
}

/**
 * Request timeout middleware
 */
function requestTimeout (timeoutMs = 30000) {
  return (req, res, next) => {
    req.setTimeout(timeoutMs, () => {
      const error = new Error('Request timeout')
      error.code = 'REQUEST_TIMEOUT'
      error.status = 408
      next(error)
    })
    next()
  }
}

/**
 * Response time tracking middleware
 */
function responseTime (req, res, next) {
  const startTime = Date.now()

  res.on('finish', () => {
    const duration = Date.now() - startTime
    res.set('X-Response-Time', `${duration}ms`)

    // Log slow requests
    if (duration > 1000) { // Log requests over 1 second
      console.warn(`⚠️ Slow request: ${req.method} ${req.url} took ${duration}ms`)
    }
  })

  next()
}

module.exports = {
  validateRequest,
  validateApiKey,
  requestTimeout,
  responseTime,
  generateRequestId,
  validateRequestSize
}
