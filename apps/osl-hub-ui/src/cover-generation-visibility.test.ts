import { afterEach, describe, expect, it, vi } from "vitest";

import { CoverGenerationVisibility } from "./cover-generation-visibility";

afterEach(() => {
  vi.useRealTimers();
});

describe("TU-81 cover-generation visibility", () => {
  it("never renders a progress surface for a warm-pool completion", () => {
    vi.useFakeTimers();
    const changes: boolean[] = [];
    const policy = new CoverGenerationVisibility({ onVisibilityChange: (visible) => changes.push(visible) });

    policy.start();
    vi.advanceTimersByTime(399);
    policy.finish();
    vi.advanceTimersByTime(601);

    expect(policy.visible).toBe(false);
    expect(changes).toEqual([]);
  });

  it("keeps a delayed progress surface visible for the minimum duration", () => {
    vi.useFakeTimers();
    const changes: boolean[] = [];
    const policy = new CoverGenerationVisibility({ onVisibilityChange: (visible) => changes.push(visible) });

    policy.start();
    vi.advanceTimersByTime(400);
    expect(policy.visible).toBe(true);
    policy.finish();
    vi.advanceTimersByTime(599);

    expect(policy.visible).toBe(true);
    expect(changes).toEqual([true]);
    vi.advanceTimersByTime(1);
    expect(policy.visible).toBe(false);
    expect(changes).toEqual([true, false]);
  });
});
