"use strict";
/**
 * ID generation utilities — UUIDs, secure random strings, hashes.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.generateId = generateId;
exports.generateSecureId = generateSecureId;
exports.hashString = hashString;
const crypto_1 = require("crypto");
/**
 * Generate a random UUID v4 using Node's crypto module.
 */
function generateId() {
    return randomUUID();
}
/**
 * Generate a cryptographically secure random ID string of given byte length.
 * Output is hex-encoded, so length is doubled.
 */
function generateSecureId(byteLength = 16) {
    return (0, crypto_1.randomBytes)(byteLength).toString('hex');
}
/**
 * Hash a string with SHA-256 and return hex-encoded result.
 */
function hashString(input, algorithm = 'sha256') {
    return (0, crypto_1.createHash)(algorithm).update(input, 'utf-8').digest('hex');
}
/** Polyfill for environments that don't have randomUUID */
function randomUUID() {
    const bytes = (0, crypto_1.randomBytes)(16);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = bytes.toString('hex');
    return [
        hex.slice(0, 8),
        hex.slice(8, 12),
        hex.slice(12, 16),
        hex.slice(16, 20),
        hex.slice(20, 32),
    ].join('-');
}
//# sourceMappingURL=id.js.map