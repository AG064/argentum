"use strict";
/**
 * Object manipulation utilities — safe JSON parsing, deep operations, etc.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.parseJsonSafe = parseJsonSafe;
exports.deepClone = deepClone;
exports.deepMerge = deepMerge;
exports.omit = omit;
exports.pick = pick;
/**
 * Parse JSON without throwing. Returns undefined on failure.
 */
function parseJsonSafe(text) {
    try {
        return JSON.parse(text);
    }
    catch {
        return undefined;
    }
}
/**
 * Deep clone any serializable value.
 */
function deepClone(value) {
    if (value === null || typeof value !== 'object')
        return value;
    if (value instanceof Date)
        return new Date(value.getTime());
    if (Array.isArray(value))
        return value.map(deepClone);
    const copy = {};
    for (const key of Object.keys(value)) {
        copy[key] = deepClone(value[key]);
    }
    return copy;
}
/**
 * Deep merge two objects. Source values override target values.
 */
function deepMerge(target, source) {
    const result = { ...target };
    for (const key of Object.keys(source)) {
        const sourceVal = source[key];
        const targetVal = target[key];
        if (sourceVal !== null &&
            typeof sourceVal === 'object' &&
            !Array.isArray(sourceVal) &&
            targetVal !== null &&
            typeof targetVal === 'object' &&
            !Array.isArray(targetVal)) {
            result[key] = deepMerge(targetVal, sourceVal);
        }
        else if (sourceVal !== undefined) {
            result[key] = sourceVal;
        }
    }
    return result;
}
/**
 * Return a new object with the specified keys omitted.
 */
function omit(obj, keys) {
    const omittedKeys = new Set(keys);
    return Object.fromEntries(Object.entries(obj).filter(([key]) => !omittedKeys.has(key)));
}
/**
 * Return a new object containing only the specified keys.
 */
function pick(obj, keys) {
    const result = {};
    for (const key of keys) {
        if (key in obj) {
            result[key] = obj[key];
        }
    }
    return result;
}
//# sourceMappingURL=object.js.map