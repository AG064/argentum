/**
 * Object manipulation utilities — safe JSON parsing, deep operations, etc.
 */
/**
 * Parse JSON without throwing. Returns undefined on failure.
 */
export declare function parseJsonSafe<T = unknown>(text: string): T | undefined;
/**
 * Deep clone any serializable value.
 */
export declare function deepClone<T>(value: T): T;
/**
 * Deep merge two objects. Source values override target values.
 */
export declare function deepMerge<T extends object>(target: T, source: Partial<T>): T;
/**
 * Return a new object with the specified keys omitted.
 */
export declare function omit<T extends Record<string, unknown>, K extends keyof T>(obj: T, keys: readonly K[]): Omit<T, K>;
/**
 * Return a new object containing only the specified keys.
 */
export declare function pick<T extends Record<string, unknown>, K extends keyof T>(obj: T, keys: readonly K[]): Pick<T, K>;
//# sourceMappingURL=object.d.ts.map