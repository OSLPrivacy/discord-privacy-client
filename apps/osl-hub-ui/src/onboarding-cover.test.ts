import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  chooseCoverInsertion,
  initialCoverInsertionChoice,
  onboardingCoverMarkup,
} from "./onboarding-cover";

const styles = readFileSync(new URL("./onboarding-cover.css", import.meta.url), "utf8");

describe("Cover insertion onboarding screen", () => {
  it("starts without a saved option", () => {
    expect(initialCoverInsertionChoice()).toBeNull();
    const markup = onboardingCoverMarkup(initialCoverInsertionChoice());
    expect(markup).not.toContain('value="insert-on-send" checked');
    expect(markup).not.toContain('value="type-naturally" checked');
  });

  it("moves the selection to either option", () => {
    expect(chooseCoverInsertion(null, "type-naturally")).toBe("type-naturally");
    expect(onboardingCoverMarkup("type-naturally")).toContain('value="type-naturally" checked');
    expect(onboardingCoverMarkup("type-naturally")).not.toContain('value="insert-on-send" checked');
  });

  it("keeps the wiring the Continue handler and the radio listener bind to", () => {
    const markup = onboardingCoverMarkup(initialCoverInsertionChoice());
    expect(markup).toContain('id="continue-cover-draft"');
    expect(markup).toContain('type="radio" name="cover-mode"');
    expect(markup).toContain('class="setup-footer onboarding-actions"');
  });

  it("names the tier on each card without making it the card's title", () => {
    const markup = onboardingCoverMarkup(initialCoverInsertionChoice());
    expect(markup).toContain('<em class="cover-tag">FREE</em>');
    expect(markup).toContain('<em class="cover-tag">PRO</em>');
    expect(markup).toContain("Insert on send");
    expect(markup).toContain("Type naturally");
  });

  it("shows the two demos instead of describing them", () => {
    const markup = onboardingCoverMarkup(initialCoverInsertionChoice());
    // The sentences the demo boxes replaced.
    expect(markup).not.toContain("Press Enter");
    expect(markup).not.toContain("one character at a time.");
    // Both demos spell the same phrase, so only the arrival differs.
    expect(markup.match(/Looks good/gu)).toHaveLength(2);
    expect(markup).toContain('class="cover-demo-text cover-demo-atomic"');
    expect(markup).toContain('class="cover-demo-clip"');
    expect(markup).toContain('class="cover-caret"');
  });

  it("blinks the atomic demo rather than fading it", () => {
    // A fade would show the cover arriving gradually, which is the OTHER
    // option's behaviour. step-end is what makes the difference visible.
    expect(styles).toContain("animation: cover-atomic 3.2s step-end infinite");
    expect(styles).toContain("steps(10, end)");
  });

  it("holds both demos filled when motion is turned off", () => {
    // A reduced-motion user must still be able to tell the options apart, and a
    // permanently empty box would read as broken.
    const reduced = styles.slice(styles.indexOf("@media (prefers-reduced-motion: reduce)"));
    expect(reduced).toContain(".cover-caret { animation: none; }");
    expect(reduced).toContain(".cover-demo-atomic { opacity: 1; }");
    expect(reduced).toContain(".cover-demo-clip { width: 10ch; }");
  });
});
