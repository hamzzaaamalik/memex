/**
 * Global error handler middleware for MindCache API
 */

/**
 * Error response formatter
 */
function formatErrorResponse (error, req) {
  const timestamp = new Date().toISOString()
  const requestId = req.id || generateRequestId()

  // Base error response
  const response = {
    error: error.message || 'Internal Server Error',
    code: error.code || 'INTERNAL_ERROR',
    timestamp,
    requestId,
    path: req.path,
    method: req.method
  }

  // Add stack trace in development
  if (process.env.NODE_ENV === 'development') {
    response.stack = error.stack
  }

  // Add additional details if available
  if (error.details) {
    response.details = error.details
  }

  return response
}

/**
 * Generate unique request ID
 */
function generateRequestId () {
  return Math.random().toString(36).substr(2, 9) + Date.now().toString(36)
}

/**
 * Log error details
 */
function logError (error, req, res) {
  const logData = {
    timestamp: new Date().toISOString(),
    method: req.method,
    url: req.url,
    userAgent: req.get('User-Agent'),
    ip: req.ip,
    error: {
      name: error.name,
      message: error.message,
      code: error.code,
      stack: error.stack
    }
  }

  // Log to console (in production, you might want to use a proper logger)
  console.error('🚨 API Error:', JSON.stringify(logData, null, 2))
}

/**
 * Main error handler middleware
 */
function errorHandler (error, req, res, next) {
  // If response already sent, delegate to default Express error handler
  if (res.headersSent) {
    return next(error)
  }

  // Log the error
  logError(error, req, res)

  // Determine status code
  let statusCode = 500

  // Handle specific error types
  if (error.name === 'ValidationError' || error.code === 'VALIDATION_ERROR') {
    statusCode = 400
  } else if (error.name === 'UnauthorizedError' || error.code === 'UNAUTHORIZED') {
    statusCode = 401
  } else if (error.code === 'FORBIDDEN') {
    statusCode = 403
  } else if (error.code === 'NOT_FOUND') {
    statusCode = 404
  } else if (error.code === 'CONFLICT') {
    statusCode = 409
  } else if (error.code === 'RATE_LIMIT_EXCEEDED') {
    statusCode = 429
  } else if (error.name === 'SyntaxError' && error.status === 400) {
    // JSON parsing error
    statusCode = 400
    error.message = 'Invalid JSON in request body'
    error.code = 'INVALID_JSON'
  } else if (error.code === 'RUST_BRIDGE_ERROR') {
    statusCode = 500
  } else if (error.status) {
    statusCode = error.status
  }

  // Format response
  const errorResponse = formatErrorResponse(error, req)

  // Send error response
  res.status(statusCode).json(errorResponse)
}

/**
 * 404 handler for unmatched routes
 */
function notFoundHandler (req, res) {
  const error = {
    error: 'Route not found',
    code: 'ROUTE_NOT_FOUND',
    message: `The requested route ${req.method} ${req.path} was not found`,
    timestamp: new Date().toISOString(),
    path: req.path,
    method: req.method
  }

  res.status(404).json(error)
}

/**
 * Async error wrapper for route handlers
 */
function asyncHandler (fn) {
  return (req, res, next) => {
    Promise.resolve(fn(req, res, next)).catch(next)
  }
}

/**
 * Create custom error
 */
function createError (message, code = 'CUSTOM_ERROR', statusCode = 500, details = null) {
  const error = new Error(message)
  error.code = code
  error.status = statusCode
  if (details) {
    error.details = details
  }
  return error
}

module.exports = {
  errorHandler,
  notFoundHandler,
  asyncHandler,
  createError,
  formatErrorResponse,
  logError
}
