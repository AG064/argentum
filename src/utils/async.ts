/**
 * Async utilities — retry, backoff, timeout, debounce.
 */

/**
 * Retry an async operation up to maxAttempts times.
 * Passes attempt number (1-based) to the operation.
 */
export async function retry<T>(
  operation: (attempt: number) => Promise<T>,
  maxAttempts = 3,
  delayMs = 0,
): Promise<T> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      return await operation(attempt);
    } catch (err) {
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
export async function retryWithBackoff<T>(
  operation: (attempt: number) => Promise<T>,
  maxAttempts = 5,
  baseDelayMs = 200,
  maxDelayMs = 30_000,
  jitterFraction = 0.2,
): Promise<T> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      return await operation(attempt);
    } catch (err) {
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
export class TimeoutError extends Error {
  readonly timedOut = true;
  constructor(readonly ms: number) {
    super(`Operation timed out after ${ms}ms`);
    this.name = 'TimeoutError';
  }
}

export async function timeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const wrapper = new Promise<T>((_, reject) => {
    timer = setTimeout(() => reject(new TimeoutError(ms)), ms);
  });
  try {
    return await Promise.race([promise, wrapper]);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * Create a debounced version of an async function.
 * The returned function delays invocation until after waitMs of inactivity.
 */
export function debounce<T extends (...args: Parameters<T>) => ReturnType<T>>(
  fn: T,
  waitMs: number,
): (...args: Parameters<T>) => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  return (...args: Parameters<T>) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      fn(...args);
    }, waitMs);
  };
}

/** Sleep for a given number of milliseconds. */
export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
