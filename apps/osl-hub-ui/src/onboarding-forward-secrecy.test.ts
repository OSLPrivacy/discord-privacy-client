import { describe, expect, it } from "vitest";
import {
  canContinuePastForwardSecrecyChoice,
  chooseForwardSecrecyMode,
  initialForwardSecrecyOnboardingState,
  onboardingForwardSecrecyMarkup,
} from "./onboarding-forward-secrecy";

describe("D111 forward-secrecy onboarding choice", () => {
  it("starts unselected and cannot continue", () => {
    const state = initialForwardSecrecyOnboardingState();
    expect(state.choice).toBeNull();
    expect(canContinuePastForwardSecrecyChoice(state)).toBe(false);
    expect(onboardingForwardSecrecyMarkup(state)).toContain("disabled");
  });

  it("states both irreversible costs and records an explicit choice", () => {
    const markup = onboardingForwardSecrecyMarkup(initialForwardSecrecyOnboardingState());
    expect(markup).toContain("A stolen data copy cannot reconstruct earlier message keys");
    expect(markup).toContain("restart begins a fresh chain and late messages are lost");
    expect(markup).toContain("persisted snapshot can recover prior message keys");
    expect(chooseForwardSecrecyMode(initialForwardSecrecyOnboardingState(), "protect-past").choice).toBe("protect-past");
    expect(chooseForwardSecrecyMode(initialForwardSecrecyOnboardingState(), "keep-group-delivery").choice).toBe("keep-group-delivery");
  });

  it("renders the same two-child choice card and shared action row as the tor step", () => {
    const markup = onboardingForwardSecrecyMarkup(initialForwardSecrecyOnboardingState());
    // A third child in `.tor-choice-card` reflows the copy under the 1.125rem
    // radio column and wraps the text one or two words per line.
    expect(markup).not.toContain("tor-choice-icon");
    expect(markup).toContain('class="compact-lead onboarding-centered-copy"');
    expect(markup).toContain('class="setup-footer onboarding-actions"');
  });
});
