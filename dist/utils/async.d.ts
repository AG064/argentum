/**
 * Async utilities — retry, backoff, timeout, debounce.
 */
/**
 * Retry an async operation up to maxAttempts times.
 * Passes attempt number (1-based) to the operation.
 */
export declare function retry<T>(operation: (attempt: number) => Promise<T>, maxAttempts?: number, delayMs?: number): Promise<T>;
/**
 * Retry with exponential backoff. Jitter fraction randomizes delay.
 */
export declare function retryWithBackoff<T>(operation: (attempt: number) => Promise<T>, maxAttempts?: number, baseDelayMs?: number, maxDelayMs?: number, jitterFraction?: number): Promise<T>;
/**
 * Wrap a promise with a timeout. Rejects with TimeoutError if exceeded.
 */
export declare class TimeoutError extends Error {
    readonly ms: number;
    readonly timedOut = true;
    constructor(ms: number);
}
export declare function timeout<T>(promise: Promise<T>, ms: number): Promise<T>;
/**
 * Create a debounced version of an async function.
 * The returned function delays invocation until after waitMs of inactivity.
 */
export declare function debounce<T extends (...args: Parameters<T>) => ReturnType<T>>(fn: T, waitMs: number): (...args: Parameters<T>) => void;
/** Sleep for a given number of milliseconds. */
export declare function sleep(ms: number): Promise<void>;
//# sourceMappingURL=async.d.ts.map