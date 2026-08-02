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
  it("ships a click-through explanation of Lock, Eye, ring, send mode, and limits", () => {
    const tour = functionSource("tutorialContent", "chooseAppsOnboardingContent");
    for (const required of ["Lock", "Eye", "cyan ring", "Send mode", "App limits"]) {
      expect(tour.toLowerCase()).toContain(required.toLowerCase());
    }
    expect(tour).toContain('id="onboarding-tour-next"');
    expect(tour).toContain('id="onboarding-tour-back"');
    const entry = functionSource("enterCombinedAppChoice", "persistCombinedHomeChoices");
    expect(entry).toContain('onboardingRoute = "tutorial"');
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
