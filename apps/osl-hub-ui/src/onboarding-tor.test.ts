import { describe, expect, it } from "vitest";

import {
  applyTorBootstrapStatus,
  canContinuePastTorChoice,
  chooseBridgeUsage,
  chooseLocalNetworkUsage,
  chooseTorRoute,
  initialTorOnboardingState,
  onboardingTorMarkup,
} from "./onboarding-tor";
import { applyTorSidecarEvent, initialTorBootStatus, markTorBootSlow } from "./tor-boot-orchestrator";

describe("Tor onboarding choice", () => {
  // TASK 5019 keeps Direct shipped until the packaged-build 4900 proof is
  // green over the tunnel. The current adjudicated proof is red.
  it("starts on Direct and can continue", () => {
    const state = initialTorOnboardingState();
    const markup = onboardingTorMarkup(state);

    expect(state.choice).toBe("direct");
    expect(state.usingBridge).toBe(false);
    expect(canContinuePastTorChoice(state)).toBe(true);
    expect(markup).not.toMatch(/value="tor"[^>]*checked/u);
    expect(markup).toMatch(/value="direct"[^>]*checked/u);
    expect(markup).not.toContain("disabled");
  });

  it("shows the shipped bridge control with the exact first-run label", () => {
    const markup = onboardingTorMarkup(chooseTorRoute(initialTorOnboardingState(), "tor"));

    expect(markup.match(/type="radio"/gu)).toHaveLength(2);
    expect(markup).toContain('<span class="tor-bridge-label">Using a bridge</span>');
    expect(markup).toContain('type="checkbox" id="tor-bridge"');
    expect(markup).not.toMatch(/id="tor-bridge"[^>]*checked/u);
  });

  it("preserves bridge use while the route and bootstrap projection change", () => {
    const enabled = chooseBridgeUsage(initialTorOnboardingState(), true);
    expect(enabled.usingBridge).toBe(true);
    expect(onboardingTorMarkup(enabled)).toMatch(/id="tor-bridge"[^>]*checked/u);

    const direct = chooseTorRoute(enabled, "direct");
    expect(direct.usingBridge).toBe(true);
    expect(onboardingTorMarkup(direct)).not.toContain('id="tor-bridge"');

    const restoredTor = chooseTorRoute(direct, "tor");
    expect(restoredTor.usingBridge).toBe(true);
    const connecting = applyTorBootstrapStatus(restoredTor, initialTorBootStatus());
    expect(connecting.usingBridge).toBe(true);
  });

  it("gives the two routes equal weight and keeps Mullvad beside them as status", () => {
    const markup = onboardingTorMarkup(initialTorOnboardingState());

    expect(markup.match(/type="radio"/gu)).toHaveLength(2);
    expect(markup).toContain('value="tor"');
    expect(markup).toContain('value="direct"');
    expect(markup).toContain(">Connection choice</h1>");
    expect(markup).toContain('aria-label="Tor"');
    expect(markup).toContain('aria-label="Direct"');
    expect(markup).toContain("<strong>Tor</strong>");
    expect(markup).toContain("<strong>Direct</strong>");
    expect(markup).toContain('<div class="tor-mullvad-status" aria-label="Mullvad status">');
    expect(markup).toContain("You can use both. Neither replaces the other.");
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

    expect(markup).not.toContain("may take longer");
    expect(markup).not.toContain("network provider can see");
    expect(markup).not.toContain("saved before OSL sends");
    expect(markup).not.toContain("tor-choice-icon");
  });

  it("switches the selection to either route", () => {
    const tor = chooseTorRoute(initialTorOnboardingState(), "tor");
    const direct = chooseTorRoute(initialTorOnboardingState(), "direct");

    expect(canContinuePastTorChoice(tor)).toBe(true);
    expect(onboardingTorMarkup(tor)).toMatch(/value="tor"[^>]*checked/u);
    expect(canContinuePastTorChoice(direct)).toBe(true);
    expect(onboardingTorMarkup(direct)).toMatch(/value="direct"[^>]*checked/u);
  });

  it("makes Tor and local network mutually exclusive and explains each change", () => {
    const torDefault = chooseTorRoute(initialTorOnboardingState(), "tor");
    const localNetwork = chooseLocalNetworkUsage(torDefault, true);
    expect(localNetwork.choice).toBe("direct");
    expect(localNetwork.localNetworkEnabled).toBe(true);
    expect(onboardingTorMarkup(localNetwork)).toContain("Tor was turned off because Tor hides where you are, while local network access needs to see your local network.");

    const tor = chooseTorRoute(localNetwork, "tor");
    expect(tor.choice).toBe("tor");
    expect(tor.localNetworkEnabled).toBe(false);
    expect(onboardingTorMarkup(tor)).toContain("Local network was turned off because Tor hides where you are, while local network access needs to see your local network.");
    expect(tor.choice === "tor" && tor.localNetworkEnabled).toBe(false);
  });

  it("proves neither control can leave both enabled, including a legacy stored-both state", () => {
    const starts = [
      initialTorOnboardingState(),
      chooseTorRoute(initialTorOnboardingState(), "direct"),
      chooseLocalNetworkUsage(initialTorOnboardingState(), true),
      { ...initialTorOnboardingState(), choice: "tor" as const, localNetworkEnabled: true },
    ];
    let checked = 0;
    for (const start of starts) {
      const torOn = chooseTorRoute(start, "tor");
      expect(torOn.choice === "tor" && torOn.localNetworkEnabled).toBe(false);
      checked++;

      const localOn = chooseLocalNetworkUsage(start, true);
      expect(localOn.choice === "tor" && localOn.localNetworkEnabled).toBe(false);
      checked++;
    }
    expect(checked).toBe(8);

    const legacy = starts[3]!;
    const repairedByTor = chooseTorRoute(legacy, "tor");
    expect(repairedByTor.localNetworkEnabled).toBe(false);
    expect(onboardingTorMarkup(repairedByTor)).toContain("Local network was turned off because Tor hides where you are, while local network access needs to see your local network.");
    const repairedByLocal = chooseLocalNetworkUsage(legacy, true);
    expect(repairedByLocal.choice).toBe("direct");
    expect(onboardingTorMarkup(repairedByLocal)).toContain("Tor was turned off because Tor hides where you are, while local network access needs to see your local network.");
  });

  it("renders first-run progress copied from a sidecar bootstrap event", () => {
    const status = applyTorSidecarEvent(initialTorBootStatus(), { event: "bootstrap", percent: 40 });
    const state = applyTorBootstrapStatus(initialTorOnboardingState(), status);
    expect(onboardingTorMarkup(state)).toContain("Connecting -- 40%");
  });

  it("keeps slow separate from failure and offers exits only on explicit error", () => {
    const slow = applyTorBootstrapStatus(initialTorOnboardingState(), markTorBootSlow(initialTorBootStatus()));
    expect(onboardingTorMarkup(slow)).toContain("Slow -- still trying");
    expect(onboardingTorMarkup(slow)).not.toContain(">Retry</button>");

    const failedStatus = applyTorSidecarEvent(initialTorBootStatus(), { event: "error", message: "no route" });
    const failed = applyTorBootstrapStatus(initialTorOnboardingState(), failedStatus);
    expect(onboardingTorMarkup(failed)).toContain("Failed -- Tor could not connect");
    expect(onboardingTorMarkup(failed)).toContain(">Retry</button>");
    expect(onboardingTorMarkup(failed)).toContain(">Direct</button>");
  });
});
