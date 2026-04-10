"use strict";
/**
 * Validation utilities — URL, path, email, and other common checks.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.isValidUrl = isValidUrl;
exports.isSafePath = isSafePath;
exports.isEmail = isEmail;
const SAFE_PATH_PATTERN = /^[a-zA-Z0-9_./-]+$/;
const EMAIL_PATTERN = /^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$/;
/**
 * Check if a string is a valid absolute URL.
 */
function isValidUrl(value) {
    if (typeof value !== 'string')
        return false;
    try {
        const url = new URL(value);
        return url.protocol === 'http:' || url.protocol === 'https:';
    }
    catch {
        return false;
    }
}
/**
 * Check if a file path looks safe (no traversal, no proto-pollution).
 * This is a heuristic — prefer deny-by-default.
 */
function isSafePath(value) {
    if (typeof value !== 'string')
        return false;
    if (value.includes('..'))
        return false;
    if (!SAFE_PATH_PATTERN.test(value))
        return false;
    return true;
}
/**
 * Check if a string is a valid email address.
 */
function isEmail(value) {
    if (typeof value !== 'string')
        return false;
    return EMAIL_PATTERN.test(value);
}
//# sourceMappingURL=validation.js.map