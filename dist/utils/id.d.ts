/**
 * ID generation utilities — UUIDs, secure random strings, hashes.
 */
/**
 * Generate a random UUID v4 using Node's crypto module.
 */
export declare function generateId(): string;
/**
 * Generate a cryptographically secure random ID string of given byte length.
 * Output is hex-encoded, so length is doubled.
 */
export declare function generateSecureId(byteLength?: number): string;
/**
 * Hash a string with SHA-256 and return hex-encoded result.
 */
export declare function hashString(input: string, algorithm?: string): string;
//# sourceMappingURL=id.d.ts.map