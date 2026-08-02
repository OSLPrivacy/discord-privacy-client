import { describe, expect, it, vi } from "vitest";
import { createViewOnceTimer } from "./view-once-timer";

describe("view-once timer", () => {
  it("does not extend the countdown when a clock sample moves backwards", () => {
    let now = 10_000;
    const timer = createViewOnceTimer({
      lifetimeMs: 5_000,
      now: () => now,
      onClose: vi.fn(),
    });

    now = 12_000;
    expect(timer.tick()).toMatchObject({ remainingMs: 3_000, remainingSeconds: 3, ringProgress: 0.6 });

    now = 11_000;
    expect(timer.tick()).toMatchObject({ remainingMs: 3_000, remainingSeconds: 3, ringProgress: 0.6 });
  });

  it("fires close exactly once when the deadline wins a manual-close race", () => {
    let now = 0;
    const onClose = vi.fn();
    const timer = createViewOnceTimer({ lifetimeMs: 1_000, now: () => now, onClose });

    now = 1_000;
    expect(timer.tick()).toMatchObject({ closed: true, remainingSeconds: 0, ringProgress: 0 });
    expect(timer.close()).toMatchObject({ closed: true, remainingSeconds: 0 });
    expect(timer.tick()).toMatchObject({ closed: true, remainingSeconds: 0 });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("makes an early manual close terminal", () => {
    let now = 100;
    const onClose = vi.fn();
    const timer = createViewOnceTimer({ lifetimeMs: 1_000, now: () => now, onClose });

    expect(timer.close()).toMatchObject({ closed: true, remainingMs: 0 });
    now = 1_100;
    timer.tick();
    expect(onClose).toHaveBeenCalledOnce();
  });
});
