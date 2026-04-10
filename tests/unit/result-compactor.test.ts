/**
 * Unit tests for ResultCompactor
 */

import { rmSync, existsSync, readFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { compactResult, compactResults } from '../../src/core/result-compactor';

const TEST_OUTPUT_DIR = join(tmpdir(), 'ag-claw-results-test');

// Clean up test artifacts
function cleanup(): void {
  try {
    rmSync(TEST_OUTPUT_DIR, { recursive: true, force: true });
  } catch {
    // ignore
  }
}

beforeAll(() => {
  cleanup();
});

afterAll(() => {
  cleanup();
});

describe('compactResult', () => {
  beforeEach(() => {
    cleanup();
  });

  it('should not compact short results under threshold', () => {
    const result = compactResult('test-tool', 'short result', 1000);
    expect(result.wasCompacted).toBe(false);
    expect(result.compact).toBe('short result');
    expect(result.savedPath).toBeUndefined();
  });

  it('should compact long results exceeding threshold', () => {
    const longString = 'this is a very long result that should be compacted because it exceeds the threshold';
    const result = compactResult('test-tool', longString, 50);
    expect(result.wasCompacted).toBe(true);
    expect(result.compact).toContain('...');
  });

  it('should save full result to file when compacting', () => {
    const longResult = 'long '.repeat(100);
    const result = compactResult('test-tool', longResult, 50, TEST_OUTPUT_DIR);
    expect(result.wasCompacted).toBe(true);
    expect(result.compact).toContain('...');
    expect(result.compact).toContain('truncated');
    expect(result.savedPath).toBeTruthy();
  });

  it('should read saved file and verify full content', () => {
    const fullContent = 'x'.repeat(500);
    const result = compactResult('test-tool', fullContent, 100, TEST_OUTPUT_DIR);

    if (result.savedPath && existsSync(result.savedPath)) {
      const savedContent = readFileSync(result.savedPath, 'utf-8');
      expect(savedContent).toBe(fullContent);
    }
  });

  it('should use default threshold of 2000 characters', () => {
    const mediumResult = 'a'.repeat(1500);
    const result = compactResult('test-tool', mediumResult);
    expect(result.wasCompacted).toBe(false);
  });

  it('should handle result exactly at threshold without compacting', () => {
    const exactResult = 'a'.repeat(2000);
    const result = compactResult('test-tool', exactResult, 2000);
    expect(result.wasCompacted).toBe(false);
  });

  it('should compact result just over threshold', () => {
    const justOver = 'a'.repeat(2001);
    const result = compactResult('test-tool', justOver, 2000);
    expect(result.wasCompacted).toBe(true);
  });

  it('should sanitize tool name for file path', () => {
    const result = compactResult('test-tool-with-special!@#chars', 'x'.repeat(500), 50, TEST_OUTPUT_DIR);
    expect(result.savedPath).toBeTruthy();
    // File path should not contain special characters
    if (result.savedPath) {
      const normalizedPath = result.savedPath.replace(/\\/g, '/');
      expect(normalizedPath).not.toMatch(/[^a-zA-Z0-9_\-.-\/]/);
    }
  });

  it('should include timestamp in saved file name', () => {
    const result = compactResult('test', 'x'.repeat(500), 50, TEST_OUTPUT_DIR);
    expect(result.savedPath).toBeTruthy();
    if (result.savedPath) {
      // Should have ISO-like timestamp format
      expect(result.savedPath).toMatch(/\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}/);
    }
  });
});

describe('compactResults', () => {
  beforeEach(() => {
    cleanup();
  });

  it('should compact multiple results at once', () => {
    const results = {
      'tool1': 'short',
      'tool2': 'a'.repeat(500),
      'tool3': 'medium length',
    };

    const compacted = compactResults(results, 100);

    expect(compacted['tool1'].wasCompacted).toBe(false);
    expect(compacted['tool2'].wasCompacted).toBe(true);
    expect(compacted['tool3'].wasCompacted).toBe(false);
  });

  it('should return empty object for empty input', () => {
    const compacted = compactResults({});
    expect(Object.keys(compacted)).toHaveLength(0);
  });

  it('should use custom threshold for all results', () => {
    const results = {
      'tool1': 'a'.repeat(50),
      'tool2': 'b'.repeat(50),
    };

    const compacted = compactResults(results, 30);

    expect(compacted['tool1'].wasCompacted).toBe(true);
    expect(compacted['tool2'].wasCompacted).toBe(true);
  });
});
