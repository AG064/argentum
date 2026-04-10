/**
 * Formatting utilities — bytes, duration, date, truncation.
 */
/**
 * Format bytes as a human-readable string (KB, MB, GB, etc.).
 */
export declare function formatBytes(bytes: number, decimals?: number): string;
/**
 * Format milliseconds as a human-readable duration string.
 */
export declare function formatDuration(ms: number): string;
/**
 * Format a Date or timestamp as an ISO string.
 */
export declare function formatDate(date: Date | number, includeTime?: boolean): string;
/**
 * Truncate a string to maxLength characters, appending ellipsis if needed.
 */
export declare function truncate(str: string, maxLength: number, ellipsis?: string): string;
//# sourceMappingURL=format.d.ts.map