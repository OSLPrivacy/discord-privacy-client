import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("guarded sending choices", () => {
  it("stores the explicit choice and never bypasses risk acceptance", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(binding).toContain('"clipboard", "double", "single"');
    expect(binding).toContain('setup.placementMode = "atomic"');
    expect(binding).toContain("canCompleteSetup(setup)");
  });

  it("makes Copy safe and keeps Enter modes explicit and experimental", () => {
    const onboarding = functionSource("sendingSetupContent", "onboardingPasswordRoleContent");
    const settings = functionSource("sendingSettingsContent", "privacySettingsContent");
    expect(onboarding).toContain("Choose how to send");
    expect(onboarding).toContain('data-send-mode="${mode}"');
    expect(onboarding).toContain("Never presses Send");
    expect(onboarding).toContain("Each account asks again");
    expect(settings).toContain("Will ask before first use");
    expect(`${onboarding}${settings}`).not.toMatch(/simulated typing|human-like|evasion/i);
  });

  it("uses one finite CSS animation with a reduced-motion final state", () => {
    expect(styles).toContain("@keyframes manual-send-flow");
    expect(styles).toMatch(/animation:\s*manual-send-flow[^;]*1 both/);
    expect(styles).toMatch(/@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.manual-send-demo i::after \{ animation: none !important; transform: none; \}/);
    expect(functionSource("manualSendingAnimationMarkup", "passwordEyeIcon")).not.toMatch(/setTimeout|setInterval|requestAnimationFrame/);
  });
});
