/**
 * Utils module — pure utility functions with no external dependencies.
 *
 * These are tree-shakeable, side-effect-free helpers used across the
 * codebase. Nothing here should import from core, channels, or features.
 */

// Re-export validators
export { isValidUrl, isSafePath, isEmail } from './validation';
export { parseJsonSafe, deepClone, deepMerge, omit, pick } from './object';
export { formatBytes, formatDuration, formatDate, truncate } from './format';
export { retry, retryWithBackoff, timeout, debounce } from './async';
export { hashString, generateId, generateSecureId } from './id';
