import { describe, expect, it, vi } from "vitest";
import { AutoScrubProgress } from "./autoscrub-progress";

describe("AutoScrub progress controls", () => {
  it("SCR-A3: stops before the next destructive action", async () => {
    const progress = new AutoScrubProgress(3);
    const remove = vi.fn(async () => undefined);

    await expect(progress.runNext(remove)).resolves.toEqual({ state: "deleted", completed: 1, total: 3 });
    progress.requestStop();

    await expect(progress.runNext(remove)).resolves.toEqual({ state: "stopped", completed: 1, total: 3 });
    expect(remove).toHaveBeenCalledTimes(1);
    expect(progress.snapshot()).toEqual({
      completed: 1,
      total: 3,
      remaining: 2,
      phase: "stopped",
      stopRequested: true,
      globalStopRequested: false,
    });
  });

  it("pauses without treating pending work as deleted, then resumes", async () => {
    const progress = new AutoScrubProgress(2);
    const remove = vi.fn(async () => undefined);

    progress.pause();
    await expect(progress.runNext(remove)).resolves.toEqual({ state: "paused", completed: 0, total: 2 });
    expect(remove).not.toHaveBeenCalled();

    progress.resume();
    await expect(progress.runNext(remove)).resolves.toEqual({ state: "deleted", completed: 1, total: 2 });
    expect(remove).toHaveBeenCalledTimes(1);
  });

  it("global stop halts every registered progress controller before its next delete", async () => {
    const first = new AutoScrubProgress(1);
    const second = new AutoScrubProgress(1);
    const removeFirst = vi.fn(async () => undefined);
    const removeSecond = vi.fn(async () => undefined);

    AutoScrubProgress.requestGlobalStop([first, second]);

    await expect(first.runNext(removeFirst)).resolves.toEqual({ state: "stopped", completed: 0, total: 1 });
    await expect(second.runNext(removeSecond)).resolves.toEqual({ state: "stopped", completed: 0, total: 1 });
    expect(removeFirst).not.toHaveBeenCalled();
    expect(removeSecond).not.toHaveBeenCalled();
    expect(first.snapshot().globalStopRequested).toBe(true);
    expect(second.snapshot().globalStopRequested).toBe(true);
  });
});
