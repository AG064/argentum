"use strict";
/**
 * Tool Result Compactor
 *
 * Truncates long tool outputs to prevent context overflow while preserving
 * full results to disk for later inspection.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.compactResult = compactResult;
exports.compactResults = compactResults;
const fs_1 = require("fs");
const path_1 = require("path");
const COMPACT_THRESHOLD = 2000;
const DEFAULT_OUTPUT_DIR = (0, path_1.join)(process.cwd(), 'data', 'ag-claw-results');
/**
 * Ensure output directory exists
 */
function ensureOutputDir(outputDir) {
    if (!(0, fs_1.existsSync)(outputDir)) {
        (0, fs_1.mkdirSync)(outputDir, { recursive: true });
    }
}
/**
 * Generate timestamp prefix for files
 */
function timestamp() {
    return new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
}
/**
 * Compact a tool result if it exceeds the threshold
 *
 * @param toolName - Name of the tool that produced the result
 * @param result - The raw result string
 * @param threshold - Character threshold before compaction (default 2000)
 * @returns CompactionResult with compact string and path to full result if saved
 */
function compactResult(toolName, result, threshold = COMPACT_THRESHOLD, outputDir = process.env.AGCLAW_RESULTS_DIR ?? DEFAULT_OUTPUT_DIR) {
    ensureOutputDir(outputDir);
    if (result.length <= threshold) {
        return {
            compact: result,
            wasCompacted: false,
        };
    }
    const timestampStr = timestamp();
    const safeToolName = toolName.replace(/[^a-zA-Z0-9_-]/g, '_');
    const fileName = `${timestampStr}_${safeToolName}.txt`;
    const filePath = (0, path_1.join)(outputDir, fileName);
    try {
        (0, fs_1.writeFileSync)(filePath, result, 'utf-8');
    }
    catch {
        // If we can't write to disk, return truncated version
        return {
            compact: `${result.slice(0, threshold)}... (truncation failed)`,
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
function compactResults(results, threshold = COMPACT_THRESHOLD, outputDir = process.env.AGCLAW_RESULTS_DIR ?? DEFAULT_OUTPUT_DIR) {
    const compacted = {};
    for (const [toolName, result] of Object.entries(results)) {
        compacted[toolName] = compactResult(toolName, result, threshold, outputDir);
    }
    return compacted;
}
//# sourceMappingURL=result-compactor.js.map