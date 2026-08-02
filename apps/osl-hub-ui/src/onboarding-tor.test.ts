import { describe, expect, it } from "vitest";

import {
  canContinuePastTorChoice,
  chooseTorRoute,
  initialTorOnboardingState,
  onboardingTorMarkup,
} from "./onboarding-tor";

describe("Tor onboarding choice", () => {
  it("has no selected route and cannot continue before an explicit choice", () => {
    const state = initialTorOnboardingState();
    const markup = onboardingTorMarkup(state);

    expect(state.choice).toBeNull();
    expect(canContinuePastTorChoice(state)).toBe(false);
    expect(markup).toContain('name="tor-route"');
    expect(markup).not.toContain(" checked");
    expect(markup).toContain("data-tor-choice-continue type=\"button\" disabled");
  });

  it("gives both routes equal choices and states each route's downside", () => {
    const markup = onboardingTorMarkup(initialTorOnboardingState());

    expect(markup).toContain('value="tor"');
    expect(markup).toContain('value="direct"');
    expect(markup).toContain("may take longer and may not work on every network");
    expect(markup).toContain("network provider can see that this device connects to OSL’s server");
    expect(markup).not.toMatch(/recommended|more private|faster/iu);
  });

  it("only enables onward travel after either route has been explicitly selected", () => {
    const tor = chooseTorRoute(initialTorOnboardingState(), "tor");
    const direct = chooseTorRoute(initialTorOnboardingState(), "direct");

    expect(canContinuePastTorChoice(tor)).toBe(true);
    expect(onboardingTorMarkup(tor)).toContain('value="tor" checked');
    expect(canContinuePastTorChoice(direct)).toBe(true);
    expect(onboardingTorMarkup(direct)).toContain('value="direct" checked');
  });
});
