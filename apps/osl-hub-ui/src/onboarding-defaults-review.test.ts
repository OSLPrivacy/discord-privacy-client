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

describe("review defaults onboarding", () => {
  const review = functionSource("reviewDefaultsOnboardingContent", "coverDraftSetupContent");

  it("shows warnings, attachment cleaning, retention, cleanup, and inherited send behavior", () => {
    expect(review).toContain("Review defaults");
    expect(review).toContain("Warn before sending");
    expect(review).toContain("Clean attachments");
    expect(review).toContain("Keep protected drafts");
    expect(review).toContain("Delete or clean up history");
    expect(review).toContain("Send behavior");
    expect(review).toContain("formatSendMode(defaultSetup.sendMode)");
  });

  it("starts destructive automation off and requires later review plus confirmation", () => {
    expect(review).toContain("Timed deletion, bulk cleanup, and account cleanup do not run during onboarding.");
    expect(review).toContain('"Delete or clean up history", "Timed deletion, bulk cleanup, and account cleanup do not run during onboarding.", "Off"');
    expect(review).toContain("No destructive action starts from setup.");
    expect(review).toContain("Cleanup requires a separate review and confirmation.");
    expect(review).not.toContain("auto-retry");
    expect(review).not.toContain("retry automatically");
    expect(review).not.toContain("Single Enter");
  });

  it("keeps implementation concepts out of first-run copy", () => {
    expect(review).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter/iu);
  });

  it("is wired between protection presets and send setup", () => {
    const content = functionSource("onboardingContent", "tutorialContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const previous = functionSource("previousSetupRoute", "bindOnboarding");
    expect(content).toContain('if (onboardingRoute === "defaults") return reviewDefaultsOnboardingContent();');
    expect(binding).toMatch(/#continue-onboarding-privacy[\s\S]*?onboardingRoute = "defaults"/);
    expect(binding).toMatch(/#continue-defaults-review[\s\S]*?onboardingRoute = "sending"/);
    expect(previous).toContain('defaults: "privacy"');
    expect(previous).toContain('sending: "defaults"');
  });
});
