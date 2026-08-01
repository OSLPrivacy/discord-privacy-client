import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("onboarding app choice", () => {
  it("renders available app choices with an explicit empty-selection skip", () => {
    const content = functionSource("chooseAppsOnboardingContent", "enterCombinedAppChoice");
    const binding = functionSource("bindOnboarding", "completeOnboarding");

    expect(content).toContain("Choose apps");
    expect(content).toContain("Pick available apps for Home, or skip this for now.");
    expect(content).toContain("Connected");
    expect(content).toContain("Seen in your browser history");
    expect(content).toContain("Other apps");
    expect(content).toContain('data-onboarding-app-choice="${app.id}"');
    expect(content).toContain('selectedOnboardingApps.size > 0 ? defaultContinueLabel : "Skip apps"');
    expect(content).toContain("Nothing opens during setup");
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?ensureNativeCatalogForAppChoice\(\)[\s\S]*?persistCombinedHomeChoices\(\)[\s\S]*?completeOnboarding\(\)/);
  });

  it("keeps the legacy tutorial route as a wrapper around the app chooser", () => {
    const wrapper = functionSource("tutorialContent", "chooseAppsOnboardingContent");

    expect(wrapper).toContain("return chooseAppsOnboardingContent()");
  });
});
