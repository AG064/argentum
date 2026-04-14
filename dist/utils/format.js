"use strict";
/**
 * Formatting utilities — bytes, duration, date, truncation.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.formatBytes = formatBytes;
exports.formatDuration = formatDuration;
exports.formatDate = formatDate;
exports.truncate = truncate;
/**
 * Format bytes as a human-readable string (KB, MB, GB, etc.).
 */
function formatBytes(bytes, decimals = 2) {
    if (bytes === 0)
        return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(decimals))} ${sizes[i]}`;
}
/**
 * Format milliseconds as a human-readable duration string.
 */
function formatDuration(ms) {
    if (ms < 1000)
        return `${ms}ms`;
    const seconds = Math.floor(ms / 1000);
    if (seconds < 60)
        return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    if (minutes < 60) {
        return remainingSeconds > 0 ? `${minutes}m ${remainingSeconds}s` : `${minutes}m`;
    }
    const hours = Math.floor(minutes / 60);
    const remainingMinutes = minutes % 60;
    if (remainingMinutes > 0) {
        return `${hours}h ${remainingMinutes}m`;
    }
    return `${hours}h`;
}
/**
 * Format a Date or timestamp as an ISO string.
 */
function formatDate(date, includeTime = true) {
    const d = typeof date === 'number' ? new Date(date) : date;
    const datePart = d.toISOString().split('T')[0] ?? '';
    if (!includeTime)
        return datePart;
    const timePart = d.toTimeString().split(' ')[0] ?? '';
    return `${datePart}T${timePart}`;
}
/**
 * Truncate a string to maxLength characters, appending ellipsis if needed.
 */
function truncate(str, maxLength, ellipsis = '...') {
    if (str.length <= maxLength)
        return str;
    return str.slice(0, maxLength - ellipsis.length) + ellipsis;
}
//# sourceMappingURL=format.js.map