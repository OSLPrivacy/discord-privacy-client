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

describe("truthful manual sending", () => {
  it("stores only the behavior this build can perform", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(binding).toContain('setup.sendMode = "manual"');
    expect(binding).toContain('setup.placementMode = "atomic"');
    expect(binding).toContain("setup.acceptedRisk = false");
  });

  it("does not offer unavailable automatic or keystroke modes", () => {
    const onboarding = functionSource("sendingSetupContent", "scrubCategoryChooserMarkup");
    const settings = functionSource("sendingSettingsContent", "privacySettingsContent");
    expect(onboarding).toContain("Send with copy & paste");
    expect(settings).toContain("Automatic sending is not available in this build.");
    expect(`${onboarding}${settings}`).not.toMatch(/Single Enter|Double Enter|Type it|Keystrokes|data-placement|data-send-mode/);
  });

  it("uses one finite CSS animation with a reduced-motion final state", () => {
    expect(styles).toContain("@keyframes manual-send-flow");
    expect(styles).toMatch(/animation:\s*manual-send-flow[^;]*1 both/);
    expect(styles).toMatch(/@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.manual-send-demo i::after \{ animation: none !important; transform: none; \}/);
    expect(functionSource("manualSendingAnimationMarkup", "passwordEyeIcon")).not.toMatch(/setTimeout|setInterval|requestAnimationFrame/);
  });
});
