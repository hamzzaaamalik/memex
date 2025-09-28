// Jest test setup file
// This file is run once before all tests

// Set test environment variables
process.env.NODE_ENV = 'test';
process.env.DATABASE_PATH = ':memory:';
process.env.LOG_LEVEL = 'warn';

// Global test timeout
jest.setTimeout(30000);

// Mock console methods for cleaner test output
global.console = {
  ...console,
  // Comment out the following line to see console.log output in tests
  log: jest.fn(),
  debug: jest.fn(),
  info: jest.fn(),
  warn: console.warn,
  error: console.error,
};

// Setup global test helpers
global.testHelper = {
  createTestMemory: (overrides = {}) => ({
    userId: 'test-user',
    sessionId: 'test-session',
    content: 'Test memory content',
    importance: 0.5,
    ...overrides
  }),

  sleep: (ms) => new Promise(resolve => setTimeout(resolve, ms)),
};