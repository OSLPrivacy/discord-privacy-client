export const NATIVE_HOST_DEADLINE_MS = 15_000;

export class NativeDeadlineError extends Error {
  constructor(readonly operation: string, readonly timeoutMs: number) {
    super(`${operation} did not finish within ${timeoutMs}ms`);
    this.name = "NativeDeadlineError";
  }
}

/**
 * Bound one native invocation without pretending it can be cancelled.
 *
 * Tauri/WebView2 may complete the underlying operation later. This wrapper
 * consumes that late result but never settles a second time, so callers can
 * invalidate their intent and queue fail-closed cleanup without stale UI state
 * being restored.
 */
export function withNativeDeadline<T>(
  operation: Promise<T>,
  label: string,
  timeoutMs = NATIVE_HOST_DEADLINE_MS,
): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    let settled = false;
    const timer = globalThis.setTimeout(() => {
      if (settled) return;
      settled = true;
      reject(new NativeDeadlineError(label, timeoutMs));
    }, timeoutMs);

    operation.then(
      (value) => {
        if (settled) return;
        settled = true;
        globalThis.clearTimeout(timer);
        resolve(value);
      },
      (failure) => {
        if (settled) return;
        settled = true;
        globalThis.clearTimeout(timer);
        reject(failure);
      },
    );
  });
}
