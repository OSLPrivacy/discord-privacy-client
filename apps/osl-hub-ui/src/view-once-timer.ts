/** A monotonic time source, normally `performance.now`. */
export type MonotonicNow = () => number;

export interface ViewOnceTimerSnapshot {
  /** Milliseconds still available to the display. */
  remainingMs: number;
  /** The familiar numeric countdown, rounded up so `1` means less than one second remains. */
  remainingSeconds: number;
  /** Portion of the original lifetime remaining, suitable for a progress ring. */
  ringProgress: number;
  closed: boolean;
}

export interface ViewOnceTimer {
  /** Take a snapshot and close once the non-extendable deadline is reached. */
  tick(): ViewOnceTimerSnapshot;
  /** End the display early. Subsequent ticks and closes are harmless. */
  close(): ViewOnceTimerSnapshot;
  closed(): boolean;
}

export interface ViewOnceTimerOptions {
  /** The lifetime supplied by the protected viewer; it is never reset here. */
  lifetimeMs: number;
  /** Injected for deterministic tests; production callers should use `performance.now`. */
  now?: MonotonicNow;
  /** Closes the display only. Native cleanup remains the authoritative native timer's job. */
  onClose(): void;
}

function finiteNonNegative(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 0;
}

/**
 * A display-only view-once countdown. It deliberately samples a monotonic
 * clock and remembers the greatest sample, so a backwards jump cannot add
 * time. The native protected viewer remains authoritative for destruction.
 */
export function createViewOnceTimer(options: ViewOnceTimerOptions): ViewOnceTimer {
  const lifetimeMs = finiteNonNegative(options.lifetimeMs);
  const now = options.now ?? (() => performance.now());
  let latestNow = finiteNonNegative(now());
  const deadline = latestNow + lifetimeMs;
  let didClose = false;

  const snapshot = (): ViewOnceTimerSnapshot => {
    const remainingMs = didClose ? 0 : Math.max(0, deadline - latestNow);
    return {
      remainingMs,
      remainingSeconds: Math.ceil(remainingMs / 1_000),
      ringProgress: lifetimeMs === 0 ? 0 : remainingMs / lifetimeMs,
      closed: didClose,
    };
  };

  const close = (): ViewOnceTimerSnapshot => {
    if (!didClose) {
      didClose = true;
      options.onClose();
    }
    return snapshot();
  };

  return {
    tick(): ViewOnceTimerSnapshot {
      latestNow = Math.max(latestNow, finiteNonNegative(now()));
      if (!didClose && latestNow >= deadline) return close();
      return snapshot();
    },
    close,
    closed: () => didClose,
  };
}
