export type AutoScrubProgressPhase = "running" | "paused" | "stopped" | "complete";

export interface AutoScrubProgressSnapshot {
  readonly completed: number;
  readonly total: number;
  readonly remaining: number;
  readonly phase: AutoScrubProgressPhase;
  readonly stopRequested: boolean;
  readonly globalStopRequested: boolean;
}

export type AutoScrubNextResult =
  | { readonly state: "deleted"; readonly completed: number; readonly total: number }
  | { readonly state: "paused" | "stopped" | "complete"; readonly completed: number; readonly total: number };

/**
 * Per-run visible progress and control state.
 *
 * The executor deliberately checks both stop flags immediately before it starts
 * a destructive action. Callers must use `runNext` for every individual delete,
 * never once for a batch.
 */
export class AutoScrubProgress {
  private completed = 0;
  private paused = false;
  private stopRequested = false;
  private globalStopRequested = false;

  constructor(private readonly total: number) {
    if (!Number.isSafeInteger(total) || total < 0) {
      throw new Error("AutoScrub total must be a non-negative safe integer");
    }
  }

  snapshot(): AutoScrubProgressSnapshot {
    const remaining = this.total - this.completed;
    return Object.freeze({
      completed: this.completed,
      total: this.total,
      remaining,
      phase: this.phase(),
      stopRequested: this.stopRequested,
      globalStopRequested: this.globalStopRequested,
    });
  }

  pause(): AutoScrubProgressSnapshot {
    if (!this.isTerminal()) this.paused = true;
    return this.snapshot();
  }

  resume(): AutoScrubProgressSnapshot {
    if (!this.isTerminal()) this.paused = false;
    return this.snapshot();
  }

  requestStop(): AutoScrubProgressSnapshot {
    this.stopRequested = true;
    this.paused = false;
    return this.snapshot();
  }

  requestGlobalStop(): AutoScrubProgressSnapshot {
    this.globalStopRequested = true;
    return this.requestStop();
  }

  static requestGlobalStop(runs: readonly AutoScrubProgress[]): readonly AutoScrubProgressSnapshot[] {
    return Object.freeze(runs.map((run) => run.requestGlobalStop()));
  }

  async runNext(remove: () => Promise<void>): Promise<AutoScrubNextResult> {
    // This guard is intentionally adjacent to the destructive call. Moving it
    // after `remove` would permit one extra deletion after a stop request.
    const beforeDelete = this.snapshot();
    if (beforeDelete.phase !== "running") return this.skippedResult(beforeDelete.phase);
    await remove();
    this.completed += 1;
    return {
      state: "deleted",
      completed: this.completed,
      total: this.total,
    };
  }

  private phase(): AutoScrubProgressPhase {
    if (this.stopRequested || this.globalStopRequested) return "stopped";
    if (this.completed === this.total) return "complete";
    return this.paused ? "paused" : "running";
  }

  private isTerminal(): boolean {
    const phase = this.phase();
    return phase === "stopped" || phase === "complete";
  }

  private skippedResult(state: Exclude<AutoScrubProgressPhase, "running">): AutoScrubNextResult {
    return { state, completed: this.completed, total: this.total };
  }
}
