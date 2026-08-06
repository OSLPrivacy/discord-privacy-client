import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start).toBeGreaterThanOrEqual(0);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("first-launch protected messaging tour", () => {
  // Protects the tour's CONTENT -- the five things it explains and the two
  // controls that step through them. This did not change on 2026-08-06; only
  // where the tour sits did. (Where it is reachable from is asserted below.)
  it("ships a click-through explanation of Lock, Eye, ring, send mode, and limits", () => {
    const tour = functionSource("tutorialContent", "chooseAppsOnboardingContent");
    for (const required of ["Lock", "Eye", "cyan ring", "Send mode", "App limits"]) {
      expect(tour.toLowerCase()).toContain(required.toLowerCase());
    }
    expect(tour).toContain('id="onboarding-tour-next"');
    expect(tour).toContain('id="onboarding-tour-back"');
  });

  // Protects the 2026-08-06 removal, which is the half a deletion normally
  // leaves unguarded: browser import used to hand off to the tour, and now
  // goes straight to the step that followed it. Nothing in first run may walk
  // a new person into the tour again.
  it("is no longer entered from first-run setup", () => {
    const entry = functionSource("enterCombinedAppChoice", "persistCombinedHomeChoices");
    expect(entry).toContain('onboardingRoute = "detected"');
    expect(entry).not.toContain('onboardingRoute = "tutorial"');
    // The one remaining SHIPPING writer of this route is the Settings replay,
    // asserted in the last test here. Nowhere else may re-enter it. Counted
    // against the shipping half of the file only: `__oslHubUiTest` also sets
    // the route, but that harness is never on a user's path.
    const shipping = source.slice(0, source.indexOf("export const __oslHubUiTest"));
    expect(shipping).not.toBe("");
    expect(shipping.match(/onboardingRoute = "tutorial"/gu) ?? []).toHaveLength(1);
    expect(functionSource("bindOnboarding", "completeOnboarding")).not.toContain('onboardingRoute = "tutorial"');
  });

  it("renders one Back per tour step and lets it leave the route at step one", () => {
    const tour = functionSource("tutorialContent", "chooseAppsOnboardingContent");
    // The tour owns backward navigation on this route, so the global
    // #onboarding-back must not also dock into its action row -- that shipped
    // two identically labelled "Back" buttons on all five steps.
    expect(tour).toContain("onboarding-step-back");
    const dock = functionSource("dockOnboardingBackControl", "renderOnboarding");
    expect(dock).toContain('primaryRow.querySelector(".onboarding-step-back")');
    // Being the only Back, it cannot be inert on the first sub-step.
    expect(tour).not.toContain('onboardingTourStep === 0 ? "disabled"');
    const binding = source.slice(source.indexOf('#onboarding-tour-back'));
    expect(binding.slice(0, 500)).toContain("previousSetupRoute(onboardingRoute)");
  });

  it("can be replayed from Settings without re-running setup", () => {
    const about = functionSource("updateSettingsContent", "bindUpdateControls");
    const bindingStart = source.indexOf("function bindUpdateControls");
    const bindingEnd = source.indexOf("async function refreshUpdateStatus", bindingStart + 1);
    expect(bindingStart).toBeGreaterThanOrEqual(0);
    expect(bindingEnd).toBeGreaterThan(bindingStart);
    const bindings = source.slice(bindingStart, bindingEnd);
    expect(about).toContain('id="replay-onboarding-tour"');
    expect(bindings).toContain('onboardingRoute = "tutorial"');
    expect(bindings).toContain('replayingOnboardingTour = true');
    expect(bindings).toContain('route = "onboarding"');
  });
});
