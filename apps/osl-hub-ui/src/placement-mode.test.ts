import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { RISK_ACKNOWLEDGEMENT, onboardingSendingMarkup } from "./onboarding-sending";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const sendingSource = readFileSync(new URL("./onboarding-sending.ts", import.meta.url), "utf8");
const sendingStyles = readFileSync(new URL("./onboarding-sending.css", import.meta.url), "utf8");

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
    expect(binding).toContain('"manual", "clipboard", "double"');
    expect(binding).toContain('setup.placementMode = "atomic"');
    expect(binding).toContain("canCompleteSetup(setup)");
  });

  // Protects: the four choices are described in ordinary words, and the advanced
  // risk copy stays on the acknowledgement instead of being smeared across the
  // list -- an unselected experimental mode is a plain sentence, not a warning
  // wall. Read from the rendered markup, which is what the owner actually sees.
  it("makes the onboarding choices ordinary and keeps advanced risk copy out of the trio", () => {
    const onboarding = onboardingSendingMarkup({ mode: "manual", riskAccepted: false, captureEnabled: false, captureApplied: false });
    const settings = functionSource("sendingSettingsContent", "privacySettingsContent");
    expect(onboarding).toContain('data-send-mode="manual"');
    expect(onboarding).toContain("<strong>Manual</strong>");
    expect(onboarding).toContain("<strong>Clipboard</strong>");
    expect(onboarding).toContain("<strong>Double Enter</strong>");
    expect(onboarding).toContain("<strong>Single Enter</strong>");
    expect(onboarding).not.toContain(RISK_ACKNOWLEDGEMENT);
    expect(onboarding).toMatch(/cannot prove where it is sending[^<]*sends nothing/iu);
    expect(settings).toContain("Will ask before first use");
    expect(`${onboarding}${settings}`).not.toMatch(/simulated typing|human-like|evasion/i);
  });

  // Protects what this protected about the deleted stepper, re-anchored on the
  // motion the screen actually has now. The Write/Encrypt/Copy keyframe it used
  // to name described one mode on a screen offering four; the four looping mode
  // scenes replaced it. So the rule is no longer "one finite animation" but the
  // thing that rule existed for: the motion is declarative CSS with no JS timer
  // behind it, and EVERY animated selector is given a static state under reduced
  // motion -- a comparison that cannot be watched must still be readable.
  it("gives every animated part of the sending screen a reduced-motion static state", () => {
    expect(sendingSource).not.toMatch(/setTimeout|setInterval|requestAnimationFrame/u);
    const reducedAt = sendingStyles.indexOf("@media (prefers-reduced-motion: reduce)");
    expect(reducedAt, "the sheet must ship a reduced-motion block").toBeGreaterThan(-1);
    const moving = sendingStyles.slice(0, reducedAt);
    const reduced = sendingStyles.slice(reducedAt);

    // Declarations only, and without @keyframes bodies, so a percent stop cannot
    // be mistaken for a rule and a comment cannot satisfy a check.
    const rules = moving
      .replace(/\/\*[\s\S]*?\*\//gu, "")
      .replace(/@keyframes[^{]*\{(?:[^{}]*\{[^{}]*\})*[^{}]*\}/gu, "");
    const animated = [...rules.matchAll(/([^{}]+)\{([^{}]*)\}/gu)]
      .filter((rule) => /animation:/u.test(rule[2]!))
      .flatMap((rule) => rule[1]!.split(",").map((selector) => selector.trim()).filter(Boolean));

    expect(animated.length, "the mode scenes must be animated in CSS").toBeGreaterThanOrEqual(4);
    expect(animated.every((selector) => selector.startsWith(".snd-"))).toBe(true);
    for (const selector of animated) expect(reduced).toContain(selector);
    expect(reduced).toMatch(/animation:\s*none/u);
    expect(reduced).toMatch(/transition:\s*none/u);
  });
});
