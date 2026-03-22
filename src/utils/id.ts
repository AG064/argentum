/**
 * ID generation utilities — UUIDs, secure random strings, hashes.
 */

import { createHash, randomBytes } from 'crypto';

/**
 * Generate a random UUID v4 using Node's crypto module.
 */
export function generateId(): string {
  return randomUUID();
}

/**
 * Generate a cryptographically secure random ID string of given byte length.
 * Output is hex-encoded, so length is doubled.
 */
export function generateSecureId(byteLength = 16): string {
  return randomBytes(byteLength).toString('hex');
}

/**
 * Hash a string with SHA-256 and return hex-encoded result.
 */
export function hashString(input: string, algorithm = 'sha256'): string {
  return createHash(algorithm).update(input, 'utf-8').digest('hex');
}

/** Polyfill for environments that don't have randomUUID */
function randomUUID(): string {
  const bytes = randomBytes(16);
  bytes[6] = (bytes[6]! & 0x0f) | 0x40;
  bytes[8] = (bytes[8]! & 0x3f) | 0x80;
  const hex = bytes.toString('hex');
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20, 32),
  ].join('-');
}
