import { afterEach, describe, expect, it, vi } from "vitest";
import { LatestOnlyRunner } from "./latest";

describe("latest-only native work", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("coalesces a burst of 100 requests into the active and latest run", async () => {
    const runner = new LatestOnlyRunner();
    let release: (() => void) | undefined;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    let runs = 0;
    const first = runner.request(async () => { runs += 1; await gate; });
    for (let index = 0; index < 99; index += 1) {
      void runner.request(async () => { runs += 1; });
    }
    expect(runs).toBe(1);
    release?.();
    await first;
    expect(runs).toBe(2);
  });

  it("drops pending work when the hosted context closes", async () => {
    const runner = new LatestOnlyRunner();
    let release: (() => void) | undefined;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    let runs = 0;
    const first = runner.request(async () => { runs += 1; await gate; });
    void runner.request(async () => { runs += 1; });
    runner.cancelPending();
    release?.();
    await first;
    expect(runs).toBe(1);
  });

  it("still runs the latest queued request after active native work rejects", async () => {
    const runner = new LatestOnlyRunner();
    let rejectActive: ((reason: Error) => void) | undefined;
    const gate = new Promise<void>((_resolve, reject) => { rejectActive = reject; });
    const runs: string[] = [];
    const first = runner.request(async () => { runs.push("active"); await gate; });
    void runner.request(async () => { runs.push("stale"); });
    void runner.request(async () => { runs.push("latest"); });

    rejectActive?.(new Error("native work failed"));

    await expect(first).rejects.toThrow("native work failed");
    expect(runs).toEqual(["active", "latest"]);
  });

  it("coalesces queued native work for one short scheduling window", async () => {
    vi.useFakeTimers();
    const runner = new LatestOnlyRunner(4);
    let release: (() => void) | undefined;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    const runs: string[] = [];
    const first = runner.request(async () => { runs.push("active"); await gate; });
    void runner.request(async () => { runs.push("stale"); });

    release?.();
    await vi.advanceTimersByTimeAsync(3);
    expect(runs).toEqual(["active"]);

    void runner.request(async () => { runs.push("latest"); });
    await vi.advanceTimersByTimeAsync(1);
    await first;
    expect(runs).toEqual(["active", "latest"]);
  });

  it("stays responsive during a paced sequence of interaction-driven layout requests", async () => {
    const runner = new LatestOnlyRunner();
    const runs: number[] = [];
    const pending: Promise<void>[] = [];
    const delay = (milliseconds: number) => new Promise<void>((resolve) => setTimeout(resolve, milliseconds));

    for (let interaction = 0; interaction < 40; interaction += 1) {
      pending.push(runner.request(async () => {
        runs.push(interaction);
        await delay(8);
      }));
      await delay(2);
    }
    await Promise.all(pending);

    expect(runs.at(-1)).toBe(39);
    expect(runs.length).toBeGreaterThan(2);
    // Coalescing must have dropped work: strictly fewer runs than requests.
    // The previous bound here was `< 24`, which is a SPEED assertion, not a
    // correctness one -- it depends on how far real setTimeout pacing drifts,
    // and a loaded GitHub runner produced 26 and failed CI. The exact
    // coalescing guarantee is asserted deterministically in the test below
    // instead of being inferred from wall-clock behaviour here.
    expect(runs.length).toBeLessThan(40);
  });

  it("coalesces a synchronous burst to exactly the first and the last request", async () => {
    // Fully deterministic, with no reliance on timer drift: hold the first
    // task open, issue the whole burst while the runner is provably busy, then
    // release. Every request after the first collapses into the latest one, so
    // the run list is exactly [0, 39] on any machine at any speed.
    const runner = new LatestOnlyRunner();
    const runs: number[] = [];
    let releaseFirst: () => void = () => {};
    const firstHeld = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });

    const pending: Promise<void>[] = [];
    pending.push(runner.request(async () => {
      runs.push(0);
      await firstHeld;
    }));

    // No await inside this loop: the runner is still executing request 0, so
    // each of these only replaces `latestTask`.
    for (let interaction = 1; interaction < 40; interaction += 1) {
      pending.push(runner.request(async () => {
        runs.push(interaction);
      }));
    }

    expect(runs).toEqual([0]);
    releaseFirst();
    await Promise.all(pending);

    expect(runs).toEqual([0, 39]);
  });
});
