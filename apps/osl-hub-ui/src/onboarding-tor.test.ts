import { describe, expect, it } from "vitest";

import {
  canContinuePastTorChoice,
  chooseTorRoute,
  initialTorOnboardingState,
  onboardingTorMarkup,
} from "./onboarding-tor";

describe("Tor onboarding choice", () => {
  // 2026-08-06: the screen used to start with nothing selected and a disabled
  // Continue. Liam's redesign starts on Tor. The route is still saved on
  // Continue, so what the backend receives is still what is on screen.
  it("starts on Tor and can continue", () => {
    const state = initialTorOnboardingState();
    const markup = onboardingTorMarkup(state);

    expect(state.choice).toBe("tor");
    expect(canContinuePastTorChoice(state)).toBe(true);
    expect(markup).toContain('value="tor" checked');
    expect(markup).not.toContain('value="direct" checked');
    expect(markup).not.toContain("disabled");
  });

  it("gives both routes equal weight and claims nothing in words", () => {
    const markup = onboardingTorMarkup(initialTorOnboardingState());

    expect(markup).toContain('value="tor"');
    expect(markup).toContain('value="direct"');
    expect(markup).not.toMatch(/recommended|more private|safer/iu);
    // The two animations carry the comparison, so each card states only its
    // own travel time and neither is described as the better option.
    expect(markup).toContain("travel time · 2–6 s");
    expect(markup).toContain("travel time · under 1 s");
  });

  it("keeps the wiring the Continue handler and the radio listener bind to", () => {
    const markup = onboardingTorMarkup(initialTorOnboardingState());

    expect(markup).toContain('type="radio" name="tor-route"');
    expect(markup).toContain('class="setup-footer onboarding-actions"');
    expect(markup).toContain("data-tor-choice-continue");
  });

  it("draws both diagrams from icon edges to icon centres", () => {
    const markup = onboardingTorMarkup(initialTorOnboardingState());

    // Both icons are centred on y=59 -- the monitor's middle and the gap between
    // the server's two slabs -- so the direct route is dead level rather than
    // drifting downhill. Both routes stop at x=50 and x=270, clear of the icons.
    expect(markup).toContain("M50 59 L103 32 L160 88 L217 32 L270 59");
    expect(markup).toContain("M50 59 L270 59");
    // The level route is the point: same y at both ends.
    const direct = /M50 (\d+) L270 (\d+)/u.exec(markup);
    expect(direct?.[1]).toBe(direct?.[2]);
    // Three relays on the Tor card, none on the direct one.
    expect(markup.match(/class="tor-relay"/gu)).toHaveLength(3);
  });

  it("draws the radio as SVG rather than a rounded border", () => {
    const markup = onboardingTorMarkup(initialTorOnboardingState());

    expect(markup).toContain('class="osl-radio"');
    expect(markup).toContain('viewBox="0 0 18 18"');
    // The native input still exists for the change listener; it is only hidden.
    expect(markup).toContain('class="sr-only" type="radio"');
  });

  it("drops the copy the diagrams replaced", () => {
    const markup = onboardingTorMarkup(initialTorOnboardingState());

    expect(markup).not.toContain("Connection choice");
    expect(markup).not.toContain("may take longer");
    expect(markup).not.toContain("network provider can see");
    expect(markup).not.toContain("saved before OSL sends");
    expect(markup).not.toContain("tor-choice-icon");
  });

  it("switches the selection to either route", () => {
    const tor = chooseTorRoute(initialTorOnboardingState(), "tor");
    const direct = chooseTorRoute(initialTorOnboardingState(), "direct");

    expect(canContinuePastTorChoice(tor)).toBe(true);
    expect(onboardingTorMarkup(tor)).toContain('value="tor" checked');
    expect(canContinuePastTorChoice(direct)).toBe(true);
    expect(onboardingTorMarkup(direct)).toContain('value="direct" checked');
  });
});
