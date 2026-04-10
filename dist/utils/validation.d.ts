/**
 * Validation utilities — URL, path, email, and other common checks.
 */
/**
 * Check if a string is a valid absolute URL.
 */
export declare function isValidUrl(value: unknown): value is string;
/**
 * Check if a file path looks safe (no traversal, no proto-pollution).
 * This is a heuristic — prefer deny-by-default.
 */
export declare function isSafePath(value: unknown): value is string;
/**
 * Check if a string is a valid email address.
 */
export declare function isEmail(value: unknown): value is string;
//# sourceMappingURL=validation.d.ts.map