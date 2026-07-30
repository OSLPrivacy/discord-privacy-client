export class LatestOnlyRunner {
  private running = false;
  private requested = false;
  private latestTask: (() => Promise<void>) | null = null;

  constructor(private readonly coalesceDelayMs = 4) {}

  cancelPending(): void {
    this.requested = false;
    this.latestTask = null;
  }

  async request(task: () => Promise<void>): Promise<void> {
    this.latestTask = task;
    this.requested = true;
    if (this.running) return;
    this.running = true;
    let failed = false;
    let failure: unknown;
    let ranTask = false;
    try {
      while (this.requested) {
        if (ranTask && this.coalesceDelayMs > 0) {
          await new Promise<void>((resolve) => {
            globalThis.setTimeout(resolve, this.coalesceDelayMs);
          });
        }
        const next = this.latestTask;
        this.latestTask = null;
        this.requested = false;
        if (next) {
          try {
            await next();
          } catch (error) {
            // A request may arrive while the active task is awaiting native
            // work. Preserve that latest request even if the active work
            // fails, then report the first failure after the queue drains.
            if (!failed) failure = error;
            failed = true;
          }
          ranTask = true;
        }
      }
    } finally {
      this.running = false;
    }
    if (failed) throw failure;
  }
}
