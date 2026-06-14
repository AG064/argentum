/**
 * Fuzz tests for input validation functions using fast-check.
 * These tests apply dynamic analysis by generating random inputs
 * to find edge cases and potential crashes.
 */

import { test } from '@jest/globals';
import * as fc from 'fast-check';

import { AllowlistManager } from '../../src/security/allowlists';
import { isValidUrl, isSafePath, isEmail } from '../../src/utils/validation';

test('Fuzz: isValidUrl should not throw on any string input', () => {
  fc.assert(
    fc.property(fc.string(), (value) => {
      expect(() => isValidUrl(value)).not.toThrow();
    }),
    { numRuns: 10000 },
  );
});

test('Fuzz: isValidUrl should not throw on empty string', () => {
  expect(() => isValidUrl('')).not.toThrow();
});

test('Fuzz: isValidUrl should return boolean for all inputs', () => {
  fc.assert(
    fc.property(fc.string(), (value) => {
      const result = isValidUrl(value);
      expect(typeof result).toBe('boolean');
    }),
    { numRuns: 10000 },
  );
});

test('Fuzz: isSafePath should not throw on any string input', () => {
  fc.assert(
    fc.property(fc.string(), (path) => {
      expect(() => isSafePath(path)).not.toThrow();
    }),
    { numRuns: 10000 },
  );
});

test('Fuzz: isSafePath should not throw on nullish values', () => {
  expect(() => isSafePath(null)).not.toThrow();
  expect(() => isSafePath(undefined)).not.toThrow();
});

test('Fuzz: isSafePath should not throw on empty string', () => {
  expect(() => isSafePath('')).not.toThrow();
});

test('Fuzz: isSafePath should handle path traversal attempts safely', () => {
  const traversalAttempts = [
    '../../../etc/passwd',
    '..\\..\\..\\windows\\system32',
    '${ENV_VAR}',
    '$(command)',
    '`whoami`',
    '\0null',
    '\nnewlines',
    '\rreturns',
    '\ttabs',
    '\x00bytes',
  ];
  traversalAttempts.forEach((path) => {
    expect(() => isSafePath(path)).not.toThrow();
  });
});

test('Fuzz: isSafePath should return boolean for all inputs', () => {
  fc.assert(
    fc.property(fc.string(), (value) => {
      const result = isSafePath(value);
      expect(typeof result).toBe('boolean');
    }),
    { numRuns: 10000 },
  );
});

test('Fuzz: isEmail should not throw on any string input', () => {
  fc.assert(
    fc.property(fc.string(), (value) => {
      expect(() => isEmail(value)).not.toThrow();
    }),
    { numRuns: 10000 },
  );
});

test('Fuzz: isEmail should not throw on empty string', () => {
  expect(() => isEmail('')).not.toThrow();
});

test('Fuzz: isEmail should return boolean for all inputs', () => {
  fc.assert(
    fc.property(fc.string(), (value) => {
      const result = isEmail(value);
      expect(typeof result).toBe('boolean');
    }),
    { numRuns: 10000 },
  );
});

test('Fuzz: isEmail should handle valid email patterns', () => {
  const validEmails = [
    'test@example.com',
    'user.name@domain.org',
    'user+tag@domain.co.uk',
    'a@b.co',
  ];
  validEmails.forEach((email) => {
    expect(isEmail(email)).toBe(true);
  });
});

test('Fuzz: AllowlistManager should not throw on any string input to isCommandAllowed', () => {
  const allowlist = new AllowlistManager();
  allowlist.loadConfig({
    commands: ['echo', 'ls', 'cat'],
    paths: ['/home/*/documents', '/tmp/*'],
    hosts: ['api.example.com', '*.github.com'],
  });
  fc.assert(
    fc.property(fc.string(), (cmd) => {
      expect(() => allowlist.isCommandAllowed(cmd)).not.toThrow();
    }),
    { numRuns: 10000 },
  );
});

test('Fuzz: AllowlistManager should not throw on any string input to isPathAllowed', () => {
  const allowlist = new AllowlistManager();
  allowlist.loadConfig({
    commands: ['echo', 'ls', 'cat'],
    paths: ['/home/*/documents', '/tmp/*'],
    hosts: ['api.example.com', '*.github.com'],
  });
  fc.assert(
    fc.property(fc.string(), (path) => {
      expect(() => allowlist.isPathAllowed(path)).not.toThrow();
    }),
    { numRuns: 10000 },
  );
});

test('Fuzz: AllowlistManager should not throw on any string input to isHostAllowed', () => {
  const allowlist = new AllowlistManager();
  allowlist.loadConfig({
    commands: ['echo', 'ls', 'cat'],
    paths: ['/home/*/documents', '/tmp/*'],
    hosts: ['api.example.com', '*.github.com'],
  });
  fc.assert(
    fc.property(fc.string(), (host) => {
      expect(() => allowlist.isHostAllowed(host)).not.toThrow();
    }),
    { numRuns: 10000 },
  );
});

test('Fuzz: AllowlistManager should handle glob patterns safely', () => {
  const globAttempts = ['*', '**', '???', '[a-z]', '{a,b,c}', '*/../*', '/**/*', '/*/*/*'];
  globAttempts.forEach((pattern) => {
    const testAllowlist = new AllowlistManager();
    testAllowlist.loadConfig({
      commands: [pattern],
      paths: [],
      hosts: [],
    });
    expect(() => testAllowlist.isCommandAllowed('anything')).not.toThrow();
  });
});

test('Fuzz: AllowlistManager should not throw on empty config', () => {
  const allowlist = new AllowlistManager();
  expect(() => allowlist.loadConfig({ commands: [], paths: [], hosts: [] })).not.toThrow();
});

test('Fuzz: AllowlistManager should not throw on malformed configs', () => {
  const malformedConfigs = [
    { commands: null, paths: [], hosts: [] },
    { commands: undefined, paths: [], hosts: [] },
    { commands: 'not-an-array', paths: [], hosts: [] },
    {},
  ];
  malformedConfigs.forEach((config) => {
    const allowlist = new AllowlistManager();
    expect(() => allowlist.loadConfig(config as any)).not.toThrow();
  });
});
