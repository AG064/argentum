"use strict";
/**
 * Async utilities — retry, backoff, timeout, debounce.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.TimeoutError = void 0;
exports.retry = retry;
exports.retryWithBackoff = retryWithBackoff;
exports.timeout = timeout;
exports.debounce = debounce;
exports.sleep = sleep;
/**
 * Retry an async operation up to maxAttempts times.
 * Passes attempt number (1-based) to the operation.
 */
async function retry(operation, maxAttempts = 3, delayMs = 0) {
    let lastError;
    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
        try {
            return await operation(attempt);
        }
        catch (err) {
            lastError = err;
            if (attempt < maxAttempts && delayMs > 0) {
                await sleep(delayMs);
            }
        }
    }
    throw lastError;
}
/**
 * Retry with exponential backoff. Jitter fraction randomizes delay.
 */
async function retryWithBackoff(operation, maxAttempts = 5, baseDelayMs = 200, maxDelayMs = 30_000, jitterFraction = 0.2) {
    let lastError;
    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
        try {
            return await operation(attempt);
        }
        catch (err) {
            lastError = err;
            if (attempt < maxAttempts) {
                const exp = Math.pow(2, attempt - 1);
                const delay = Math.min(exp * baseDelayMs, maxDelayMs);
                const jitter = delay * jitterFraction * Math.random();
                await sleep(delay + jitter);
            }
        }
    }
    throw lastError;
}
/**
 * Wrap a promise with a timeout. Rejects with TimeoutError if exceeded.
 */
class TimeoutError extends Error {
    ms;
    timedOut = true;
    constructor(ms) {
        super(`Operation timed out after ${ms}ms`);
        this.ms = ms;
        this.name = 'TimeoutError';
    }
}
exports.TimeoutError = TimeoutError;
async function timeout(promise, ms) {
    let timer;
    const wrapper = new Promise((_, reject) => {
        timer = setTimeout(() => reject(new TimeoutError(ms)), ms);
    });
    try {
        return await Promise.race([promise, wrapper]);
    }
    finally {
        clearTimeout(timer);
    }
}
/**
 * Create a debounced version of an async function.
 * The returned function delays invocation until after waitMs of inactivity.
 */
function debounce(fn, waitMs) {
    let timer = null;
    return (...args) => {
        if (timer)
            clearTimeout(timer);
        timer = setTimeout(() => {
            timer = null;
            fn(...args);
        }, waitMs);
    };
}
/** Sleep for a given number of milliseconds. */
function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}
//# sourceMappingURL=async.js.map