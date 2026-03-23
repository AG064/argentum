/** @type {import('jest').Config} */
module.exports = {
  testEnvironment: 'node',
  roots: ['<rootDir>/tests', '<rootDir>/src'],
  testMatch: [
    '**/__tests__/**/*.ts',
    '**/*.test.ts',
    '**/*.spec.ts',
  ],
  testPathIgnorePatterns: [
    '/node_modules/',
    '/dist/',
    '/coverage/',
    '/data/',
  ],
  transform: {
    '^.+\\.tsx?$': ['ts-jest', { useESM: false }],
  },

  transformIgnorePatterns: [
    '/node_modules/(?!(pino)/)',
  ],
  moduleNameMapper: {
    '^@/(.*)$': '<rootDir>/src/$1',
    '^@core/(.*)$': '<rootDir>/src/core/$1',
    '^@channels/(.*)$': '<rootDir>/src/channels/$1',
    '^@features/(.*)$': '<rootDir>/src/features/$1',
    '^@utils/(.*)$': '<rootDir>/src/utils/$1',
    '^@types/(.*)$': '<rootDir>/src/types/$1',
  },
  collectCoverageFrom: [
    'src/**/*.ts',
    '!src/**/*.d.ts',
    '!src/**/__tests__/**',
    '!src/**/index.ts',
    '!src/cli.ts',
    '!src/features/browser-automation/**',
  ],
  coverageThreshold: {
    global: {
      branches: 0,
      functions: 0,
      lines: 0,
      statements: 0,
    },
  },
  coverageReporters: ['text', 'text-summary', 'lcov', 'html', 'json'],
  coverageDirectory: 'coverage',
  coveragePathIgnorePatterns: [
    '/node_modules/',
    '/dist/',
  ],
  setupFilesAfterEnv: [],
  globalSetup: undefined,
  globalTeardown: undefined,
  clearMocks: true,
  restoreMocks: true,
  resetMocks: false,
  testTimeout: 30_000,
  verbose: true,
  errorOnDeprecated: true,
  passWithNoTests: true,
};
