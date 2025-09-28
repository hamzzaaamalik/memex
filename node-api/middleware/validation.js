const joi = require('joi')

/**
 * Validation schemas using Joi
 */
const schemas = {
  memorySave: joi.object({
    userId: joi.string().required().min(1).max(255),
    sessionId: joi.string().required().min(1).max(255),
    content: joi.string().required().min(1).max(100000), // 100KB max content
    metadata: joi.object().optional(),
    importance: joi.number().min(0).max(1).optional(),
    ttlHours: joi.number().integer().min(1).max(8760).optional() // Max 1 year
  }),

  memoryRecall: joi.object({
    userId: joi.string().required().min(1).max(255),
    query: joi.string().optional().max(1000),
    sessionId: joi.string().optional().max(255),
    dateFrom: joi.string().isoDate().optional(),
    dateTo: joi.string().isoDate().optional(),
    limit: joi.number().integer().min(1).max(1000).optional(),
    minImportance: joi.number().min(0).max(1).optional(),
    keywords: joi.array().items(joi.string().max(100)).max(10).optional()
  }),

  memorySummarize: joi.object({
    sessionId: joi.string().required().min(1).max(255)
  }),

  sessionCreate: joi.object({
    userId: joi.string().required().min(1).max(255),
    name: joi.string().optional().max(500),
    metadata: joi.object().optional()
  }),

  sessionSearch: joi.object({
    userId: joi.string().required().min(1).max(255),
    keywords: joi.array().items(joi.string().max(100)).min(1).max(10).required()
  }),

  bulkMemories: joi.object({
    memories: joi.array().items(
      joi.object({
        userId: joi.string().required().min(1).max(255),
        sessionId: joi.string().required().min(1).max(255),
        content: joi.string().required().min(1).max(100000),
        metadata: joi.object().optional(),
        importance: joi.number().min(0).max(1).optional(),
        ttlHours: joi.number().integer().min(1).max(8760).optional()
      })
    ).min(1).max(100).required() // Max 100 memories per batch
  })
}

/**
 * Generic validation middleware factory
 */
function createValidator (schema) {
  return (req, res, next) => {
    const { error, value } = schema.validate(req.body, {
      abortEarly: false,
      stripUnknown: true,
      convert: true
    })

    if (error) {
      const details = error.details.map(detail => ({
        field: detail.path.join('.'),
        message: detail.message,
        value: detail.context?.value
      }))

      return res.status(400).json({
        error: 'Validation failed',
        code: 'VALIDATION_ERROR',
        details,
        timestamp: new Date().toISOString()
      })
    }

    // Replace req.body with validated and sanitized data
    req.body = value
    next()
  }
}

/**
 * Query parameter validation
 */
function validateSearchQuery (req, res, next) {
  const querySchema = joi.object({
    userId: joi.string().required().min(1).max(255),
    q: joi.string().optional().max(1000),
    session: joi.string().optional().max(255),
    from: joi.string().isoDate().optional(),
    to: joi.string().isoDate().optional(),
    limit: joi.number().integer().min(1).max(1000).optional(),
    importance: joi.number().min(0).max(1).optional(),
    tags: joi.string().optional().max(1000) // Comma-separated tags
  })

  const { error, value } = querySchema.validate(req.query, {
    abortEarly: false,
    stripUnknown: true,
    convert: true
  })

  if (error) {
    const details = error.details.map(detail => ({
      field: detail.path.join('.'),
      message: detail.message,
      value: detail.context?.value
    }))

    return res.status(400).json({
      error: 'Query validation failed',
      code: 'QUERY_VALIDATION_ERROR',
      details,
      timestamp: new Date().toISOString()
    })
  }

  req.query = value
  next()
}

/**
 * User ID parameter validation
 */
function validateUserId (req, res, next) {
  const userIdSchema = joi.string().required().min(1).max(255)
  const { error, value } = userIdSchema.validate(req.params.userId)

  if (error) {
    return res.status(400).json({
      error: 'Invalid user ID',
      code: 'INVALID_USER_ID',
      message: error.details[0].message,
      timestamp: new Date().toISOString()
    })
  }

  req.params.userId = value
  next()
}

/**
 * Session ID parameter validation
 */
function validateSessionId (req, res, next) {
  const sessionIdSchema = joi.string().required().min(1).max(255)
  const { error, value } = sessionIdSchema.validate(req.params.sessionId)

  if (error) {
    return res.status(400).json({
      error: 'Invalid session ID',
      code: 'INVALID_SESSION_ID',
      message: error.details[0].message,
      timestamp: new Date().toISOString()
    })
  }

  req.params.sessionId = value
  next()
}

/**
 * Content-Type validation
 */
function validateContentType (req, res, next) {
  if (req.method === 'POST' || req.method === 'PUT') {
    if (!req.is('application/json')) {
      return res.status(415).json({
        error: 'Unsupported Media Type',
        code: 'UNSUPPORTED_MEDIA_TYPE',
        message: 'Content-Type must be application/json',
        timestamp: new Date().toISOString()
      })
    }
  }
  next()
}

/**
 * Export validation middleware
 */
module.exports = {
  validateMemorySave: createValidator(schemas.memorySave),
  validateMemoryRecall: createValidator(schemas.memoryRecall),
  validateMemorySummarize: createValidator(schemas.memorySummarize),
  validateSessionCreate: createValidator(schemas.sessionCreate),
  validateSessionSearch: createValidator(schemas.sessionSearch),
  validateBulkMemories: createValidator(schemas.bulkMemories),
  validateSearchQuery,
  validateUserId,
  validateSessionId,
  validateContentType,
  schemas
}
