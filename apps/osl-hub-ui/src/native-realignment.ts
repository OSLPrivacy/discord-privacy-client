/**
 * Bounded scheduling for the native-surface realignment path.
 *
 * The borrowed Discord window is repositioned from the renderer: every OSL move
 * or resize ends in a `resize_native_app_window` round trip. Windows delivers
 * WM_MOVE continuously while a caption is dragged, and the whole drag runs
 * inside a modal message loop on the same thread that has to service that IPC.
 * One realignment per move event is therefore not "a little more work" -- an
 * erratic wave of the mouse starves the loop that is drawing the drag, and the
 * entire app stops responding until the backlog clears.
 *
 * Two independent bounds are needed here, and they close different holes.
 *
 *  1. COALESCING (`CoalescedRealignment`). Requests that arrive while a pass is
 *     running collapse into one trailing pass. Intermediate positions are
 *     DROPPED, never queued: only the last one is a position anything will rest
 *     at. The trailing pass is guaranteed, so the resting position always wins.
 *
 *  2. PACING (`CoalescedRealignment`, same class). Coalescing alone still lets
 *     the trailing pass re-enter with a zero gap for as long as requests keep
 *     arriving -- which, during a drag, is the whole drag and one pass beyond
 *     it. A minimum interval between consecutive passes turns that saturating
 *     chain into a low-rate heartbeat and leaves the event loop room to breathe
 *     between round trips.
 *
 *  3. OUTSTANDING-CALL GATING (`NativeCallGate`). A deadline abandons a native
 *     call; it cannot cancel it. Without a gate, a pass that gives up during a
 *     stall immediately issues another invoke of the same command while the
 *     first is still genuinely running in the backend, and every further pass
 *     adds one more -- the number of outstanding IPC calls then grows for as
 *     long as the stall lasts, which is precisely the runaway this module
 *     exists to make impossible.
 *
 * Nothing here reads, logs, persists or transports draft or transcript content:
 * it schedules geometry calls and counts them.
 */

/**
 * Minimum gap between two consecutive realignment passes while requests keep
 * arriving. Sized to be invisible at rest (one settling pass ~an eighth of a
 * second after the last move) while capping a sustained drag at a handful of
 * round trips per second instead of as many as the event loop can retire.
 */
export const NATIVE_REALIGNMENT_PACING_MS = 120;

/**
 * A deliberately low-rate escape hatch for an animation frame that is paused
 * while its window is occluded or minimised. It is not a geometry poll: it is
 * armed only for an already-requested frame, and stops as soon as that frame
 * is serviced.
 */
export const NATIVE_REALIGNMENT_HEARTBEAT_MS = 1_000;

export interface CoalescedRealignmentOptions {
  /** Minimum interval between consecutive passes. 0 disables pacing. */
  readonly pacingMs?: number;
  /** Injectable clock delay, for tests. */
  readonly wait?: (ms: number) => Promise<void>;
  /** Delay before retrying a request whose animation frame is paused. */
  readonly heartbeatMs?: number;
  /** Injectable timer functions, so the paused-frame path is testable. */
  readonly setHeartbeatTimer?: (callback: () => void, ms: number) => unknown;
  readonly clearHeartbeatTimer?: (timer: unknown) => void;
}

function defaultWait(ms: number): Promise<void> {
  return new Promise<void>((resolve) => {
    globalThis.setTimeout(resolve, ms);
  });
}

/**
 * Runs one pass at a time, collapsing every request that arrives meanwhile into
 * a single trailing pass, and refusing to start consecutive passes faster than
 * `pacingMs`.
 */
export class CoalescedRealignment {
  private busy = false;
  private pending = false;
  private running = 0;
  private readonly pacingMs: number;
  private readonly wait: (ms: number) => Promise<void>;
  private readonly heartbeatMs: number;
  private readonly setHeartbeatTimer: (callback: () => void, ms: number) => unknown;
  private readonly clearHeartbeatTimer: (timer: unknown) => void;
  private heartbeatTimer: unknown | undefined;

  /** Passes actually executed. Diagnostic counters only; never persisted. */
  passes = 0;
  /** Requests collapsed into an already running pass rather than queued. */
  dropped = 0;
  /** Highest number of passes ever running at once. Must never exceed 1. */
  peakConcurrentPasses = 0;

  constructor(
    private readonly pass: () => Promise<void>,
    options: CoalescedRealignmentOptions = {},
  ) {
    this.pacingMs = options.pacingMs ?? NATIVE_REALIGNMENT_PACING_MS;
    this.wait = options.wait ?? defaultWait;
    this.heartbeatMs = options.heartbeatMs ?? NATIVE_REALIGNMENT_HEARTBEAT_MS;
    this.setHeartbeatTimer = options.setHeartbeatTimer ?? ((callback, ms) => globalThis.setTimeout(callback, ms));
    this.clearHeartbeatTimer = options.clearHeartbeatTimer ?? ((timer) => globalThis.clearTimeout(timer as ReturnType<typeof globalThis.setTimeout>));
  }

  get active(): boolean {
    return this.busy;
  }

  get trailingPassOwed(): boolean {
    return this.pending;
  }

  /**
   * Backstop one scheduled animation-frame request. A browser may pause rAF
   * indefinitely while a native child is still attached; the timer reuses
   * `request`, so its work remains coalesced and paced rather than becoming an
   * independent source of native calls.
   */
  armHeartbeat(): void {
    if (this.heartbeatTimer !== undefined) return;
    const beat = (): void => {
      this.heartbeatTimer = undefined;
      void this.request();
      this.armHeartbeat();
    };
    this.heartbeatTimer = this.setHeartbeatTimer(beat, this.heartbeatMs);
  }

  /** Stop the backstop once the animation-frame callback finally runs. */
  acknowledgeAnimationFrame(): void {
    if (this.heartbeatTimer === undefined) return;
    this.clearHeartbeatTimer(this.heartbeatTimer);
    this.heartbeatTimer = undefined;
  }

  /**
   * Ask for the native surfaces to be realigned.
   *
   * Resolves when this caller's work is accounted for: immediately if a pass is
   * already running (that pass owes a trailing pass now), otherwise once the
   * chain it started has drained.
   */
  async request(): Promise<void> {
    if (this.busy) {
      // DROP, never queue. The running pass has not committed to a position
      // yet, and the one this request carries is already stale by the time a
      // queued call would reach the backend.
      this.pending = true;
      this.dropped += 1;
      return;
    }
    this.busy = true;
    try {
      do {
        // Cleared BEFORE the pass, never after: anything observed from here on
        // -- during the pass or during the pacing gap -- earns one more pass,
        // which is what makes the resting position impossible to lose.
        this.pending = false;
        this.running += 1;
        this.peakConcurrentPasses = Math.max(this.peakConcurrentPasses, this.running);
        this.passes += 1;
        try {
          await this.pass();
        } catch {
          // A pass that throws must not cancel the trailing pass; the resting
          // position still has to be applied. The pass itself is responsible
          // for its own reporting.
        } finally {
          this.running -= 1;
        }
        if (this.pending && this.pacingMs > 0) await this.wait(this.pacingMs);
      } while (this.pending);
    } finally {
      this.busy = false;
    }
  }
}

/**
 * At most one outstanding native call per key.
 *
 * Callers that arrive while a call is outstanding share its promise instead of
 * issuing a second invoke, so a deadline that gives up on a slow call can never
 * multiply into a backlog of concurrent ones.
 */
export class NativeCallGate {
  private readonly outstanding = new Map<string, Promise<unknown>>();

  /** Number of calls currently outstanding. */
  get size(): number {
    return this.outstanding.size;
  }

  run<T>(key: string, start: () => Promise<T>): Promise<T> {
    const existing = this.outstanding.get(key) as Promise<T> | undefined;
    if (existing) return existing;
    const call = start();
    this.outstanding.set(key, call);
    const release = (): void => {
      if (this.outstanding.get(key) === call) this.outstanding.delete(key);
    };
    call.then(release, release);
    return call;
  }
}
