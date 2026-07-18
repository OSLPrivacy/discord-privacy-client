import { describe, expect, it } from "vitest";
import { LatestOnlyRunner } from "./latest";

describe("latest-only native work", () => {
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
    expect(runs.length).toBeLessThan(24);
  });
});
