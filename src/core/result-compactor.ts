/**
 * Tool Result Compactor
 *
 * Truncates long tool outputs to prevent context overflow while preserving
 * full results to disk for later inspection.
 */

import { writeFileSync, mkdirSync, existsSync } from 'fs';
import { join } from 'path';

const COMPACT_THRESHOLD = 2000;
const OUTPUT_DIR = '/tmp/ag-claw-results';

/**
 * Ensure output directory exists
 */
function ensureOutputDir(): void {
  if (!existsSync(OUTPUT_DIR)) {
    mkdirSync(OUTPUT_DIR, { recursive: true });
  }
}

/**
 * Generate timestamp prefix for files
 */
function timestamp(): string {
  return new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
}

/**
 * Save full result to disk and return compact version
 */
export interface CompactionResult {
  compact: string;
  savedPath?: string;
  wasCompacted: boolean;
}

/**
 * Compact a tool result if it exceeds the threshold
 *
 * @param toolName - Name of the tool that produced the result
 * @param result - The raw result string
 * @param threshold - Character threshold before compaction (default 2000)
 * @returns CompactionResult with compact string and path to full result if saved
 */
export function compactResult(
  toolName: string,
  result: string,
  threshold: number = COMPACT_THRESHOLD,
): CompactionResult {
  ensureOutputDir();

  if (result.length <= threshold) {
    return {
      compact: result,
      wasCompacted: false,
    };
  }

  const timestampStr = timestamp();
  const safeToolName = toolName.replace(/[^a-zA-Z0-9_-]/g, '_');
  const fileName = `${timestampStr}_${safeToolName}.txt`;
  const filePath = join(OUTPUT_DIR, fileName);

  try {
    writeFileSync(filePath, result, 'utf-8');
  } catch {
    // If we can't write to disk, return truncated version
    return {
      compact: result.slice(0, threshold) + '... (truncation failed)',
      wasCompacted: true,
    };
  }

  return {
    compact: `${result.slice(0, threshold)}...\n(truncated, full output in ${filePath})`,
    savedPath: filePath,
    wasCompacted: true,
  };
}

/**
 * Compact multiple tool results
 *
 * @param results - Record of toolName -> result strings
 * @param threshold - Character threshold per result
 * @returns Record of toolName -> CompactionResult
 */
export function compactResults(
  results: Record<string, string>,
  threshold: number = COMPACT_THRESHOLD,
): Record<string, CompactionResult> {
  const compacted: Record<string, CompactionResult> = {};

  for (const [toolName, result] of Object.entries(results)) {
    compacted[toolName] = compactResult(toolName, result, threshold);
  }

  return compacted;
}
