import { describe, expect, it } from "vitest";

import { CoverGenerationProgress, type CoverGenerationPresentationState } from "./cover-generation-progress";

describe("TU-82 cover-generation T13 signal consumer", () => {
  it("renders T13's stalled signal as a visible, non-animating generating state", () => {
    const changes: CoverGenerationPresentationState[] = [];
    const progress = new CoverGenerationProgress({ onStateChange: (state) => changes.push(state) });

    progress.consume({ type: "stage", stage: "generating", progress: 42 });
    progress.consume({ type: "stage", stage: "stalled", progress: 42 });

    expect(progress.state).toEqual({
      status: "generating",
      stage: "stalled",
      progress: 42,
      isStalled: true,
      progressAnimation: "stopped",
    });
    expect(changes).toHaveLength(2);
  });

  it("maps terminal outcomes from T13 without inventing additional progress", () => {
    const changes: CoverGenerationPresentationState[] = [];
    const progress = new CoverGenerationProgress({ onStateChange: (state) => changes.push(state) });

    progress.consume({ type: "terminal", outcome: "succeeded" });
    progress.consume({ type: "terminal", outcome: "fell-back" });
    progress.consume({ type: "terminal", outcome: "failed" });

    expect(changes).toEqual([{ status: "idle" }, { status: "fell-back" }, { status: "failed" }]);
  });
});
