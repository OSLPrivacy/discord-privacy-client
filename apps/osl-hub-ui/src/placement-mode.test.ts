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
    expect(settings).toContain("Will ask before first use");
    expect(`${onboarding}${settings}`).not.toMatch(/simulated typing|human-like|evasion/i);
  });

  it("loops centered sequential keypress animations with a reduced-motion final state", () => {
    const animationMarkup = functionSource("manualSendingAnimationMarkup", "passwordEyeIcon");
    expect(animationMarkup).toContain('["Enter", "Ctrl+V", "Enter"]');
    expect(animationMarkup).toContain('["Enter", "Enter"]');
    expect(animationMarkup).toContain('normalized === "double"');
    expect(animationMarkup).toContain('send-method-demo-${normalized}');
    expect(animationMarkup).toContain('<kbd class="send-demo-key send-demo-key-${index + 1}"');
    expect(styles).toContain("@keyframes send-key-press");
    expect(styles).toMatch(/\.send-demo-key-1\s*\{\s*animation:\s*send-key-press[^;]*infinite both/);
    expect(styles).toMatch(/\.send-demo-key-2\s*\{\s*animation:\s*send-key-press[^;]*\.56s[^;]*infinite both/);
    expect(styles).toMatch(/\.send-demo-key-3\s*\{\s*animation:\s*send-key-press[^;]*1\.12s[^;]*infinite both/);
    expect(styles).toMatch(/\.send-demo-key\s*\{[^}]*width:\s*58px;[^}]*height:\s*38px/);
    expect(styles).toMatch(/\.send-choice > button > \.send-method-demo\s*\{[^}]*width:\s*100%;[^}]*margin-top:\s*auto;[^}]*place-items:\s*center;[^}]*justify-content:\s*center;[^}]*align-content:\s*center/);
    expect(styles).toMatch(/\.send-demo-key-sequence\s*\{[^}]*width:\s*max-content;[^}]*margin-inline:\s*auto;[^}]*justify-content:\s*center/);
    expect(styles).toMatch(/@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.send-method-demo \*,[\s\S]*?animation: none !important/);
    expect(animationMarkup).not.toMatch(/setTimeout|setInterval|requestAnimationFrame/);
  });
});
