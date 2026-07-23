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
    expect(onboarding).toContain('option("clipboard", "Copy", "safe", "Safest")');
    expect(onboarding).toContain("Can possibly break ToS");
    expect(onboarding).toContain("Breaks some ToS · risky");
    expect(settings).toContain("Will ask before first use");
    expect(`${onboarding}${settings}`).not.toMatch(/simulated typing|human-like|evasion/i);
  });

  it("plays a centered sequential step animation with a reduced-motion final state", () => {
    const animationMarkup = functionSource("manualSendingAnimationMarkup", "passwordEyeIcon");
    expect(animationMarkup).toContain('class="manual-send-demo" data-send-demo="${mode}"');
    expect(animationMarkup).toContain('step(1, "Write")');
    expect(animationMarkup).toContain('step(2, "Encrypt")');
    expect(animationMarkup).toContain('mode === "clipboard" || mode === "manual" ? "Copy" : "Verify"');
    expect(animationMarkup).toContain('mode === "double" ? "Enter again" : mode === "single" ? "Recheck & send" : "You send"');
    expect(styles).toMatch(/\.manual-send-demo span\s*\{[^}]*animation:\s*manual-send-step[^;]*both/);
    expect(styles).toMatch(/\.manual-send-demo span:nth-of-type\(2\)\s*\{\s*animation-delay:\s*\.78s/);
    expect(styles).toMatch(/\.manual-send-demo span:nth-of-type\(3\)\s*\{\s*animation-delay:\s*1\.56s/);
    expect(styles).toMatch(/\.manual-send-demo span:nth-of-type\(4\)\s*\{\s*animation-delay:\s*2\.34s/);
    expect(styles).toMatch(/\.manual-send-demo\s*\{[^}]*width:\s*100%;[^}]*min-width:\s*0;[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)\s+8px\s+minmax\(0,\s*1fr\)/);
    expect(styles).toMatch(/\.manual-send-demo span\s*\{[^}]*min-width:\s*0;[^}]*flex-direction:\s*column/);
    expect(styles).toMatch(/@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.manual-send-demo span,[\s\S]*?animation: none !important/);
    expect(animationMarkup).not.toMatch(/setTimeout|setInterval|requestAnimationFrame/);
  });
});
