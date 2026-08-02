import { describe, expect, it } from "vitest";

import { renderCoverGenerationFallback } from "./cover-generation-fallback";
import { CoverGenerationProgress } from "./cover-generation-progress";

describe("TU-83 visible cover-generation fallback", () => {
  it("renders a neutral notice that names the word-bank carrier after a forced generation failure", () => {
    const progress = new CoverGenerationProgress({ onStateChange: () => {} });
    progress.consume({ type: "terminal", outcome: "failed" });

    const notice = renderCoverGenerationFallback(progress.state);

    expect(notice).toContain('data-cover-generation-fallback="word-bank"');
    expect(notice).toContain('data-honest-tone="neutral"');
    expect(notice).toContain("Cover text generation failed");
    expect(notice).toContain("word-bank carrier");
  });

  it("does not present a notice while cover text generation is still in progress", () => {
    expect(renderCoverGenerationFallback({
      status: "generating",
      stage: "generating",
      progress: 50,
      isStalled: false,
      progressAnimation: "running",
    })).toBeNull();
  });
});
