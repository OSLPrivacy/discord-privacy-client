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

describe("onboarding protection presets", () => {
  const presetContent = functionSource("protectionPresetOnboardingContent", "mullvadSetupContent");

  it("offers only the approved Basic, Balanced, and Maximum choices", () => {
    expect(presetContent).toContain('data-protection-preset="${preset.id}"');
    expect(presetContent).toContain('id: "basic"');
    expect(presetContent).toContain('title: "Basic"');
    expect(presetContent).toContain('id: "balanced"');
    expect(presetContent).toContain('title: "Balanced"');
    expect(presetContent).toContain('id: "maximum"');
    expect(presetContent).toContain('title: "Maximum"');
    expect(presetContent).not.toContain("Custom");
    expect(presetContent).not.toContain("Manual configuration");
  });

  it("selects Balanced by default and marks it as recommended", () => {
    expect(presetContent).toContain('const selected = preset.id === "balanced"');
    expect(presetContent).toContain('badge: "Recommended"');
    expect(presetContent).toContain("Balanced starts on and is safe without more setup.");
    expect(presetContent).toContain('type="radio"');
    expect(presetContent).toContain('name="protection-preset"');
    expect(presetContent).toContain('value="${preset.id}" ${selected ? "checked" : ""}');
  });

  it("states the required behavior for each preset without exposing implementation concepts", () => {
    expect(presetContent).toContain("Account health, email tracker blocking, attachment metadata warnings, and exposure alerts.");
    expect(presetContent).toContain("Basic plus local before-send warnings, one-click attachment cleaning, monthly cleanup review, and OSL protection suggestions for verified contacts.");
    expect(presetContent).toContain("Balanced plus stricter public-post checks, optional VPN-required actions, and OSL protection required for chosen contacts.");
    for (const forbidden of ["keyserver", "ratchet", "receipt", "browser profile", "provider adapter"]) {
      expect(presetContent.toLowerCase()).not.toContain(forbidden);
    }
  });

  it("keeps destructive automation off and fails closed without authority", () => {
    expect(presetContent).toContain("Deletion automation starts off. You review first.");
    expect(presetContent).toContain("No consent, account binding, or send/delete authority means Unavailable.");
    expect(presetContent).toContain("Fail closed");
    expect(presetContent).not.toContain("auto-delete");
    expect(presetContent).not.toContain("automatic deletion");
  });

  it("renders from the onboarding privacy step and continues to send setup", () => {
    const privacy = functionSource("onboardingPrivacyContent", "protectionPresetOnboardingContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(privacy).toContain("return protectionPresetOnboardingContent();");
    expect(presetContent).toContain('id="continue-onboarding-privacy"');
    expect(binding).toContain('"#continue-onboarding-privacy"');
    expect(binding).toContain('onboardingRoute = "sending"');
  });
});
