/**
 * Tool Result Compactor
 *
 * Truncates long tool outputs to prevent context overflow while preserving
 * full results to disk for later inspection.
 */
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
export declare function compactResult(toolName: string, result: string, threshold?: number, outputDir?: string): CompactionResult;
/**
 * Compact multiple tool results
 *
 * @param results - Record of toolName -> result strings
 * @param threshold - Character threshold per result
 * @returns Record of toolName -> CompactionResult
 */
export declare function compactResults(results: Record<string, string>, threshold?: number, outputDir?: string): Record<string, CompactionResult>;
//# sourceMappingURL=result-compactor.d.ts.map