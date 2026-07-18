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

describe("plain-language placement choices", () => {
  it("keeps migrated values internal while displaying plain placement language", () => {
    const choices = source.slice(source.indexOf("const placementModes"), source.indexOf("let services"));
    expect(choices).toContain('{ id: "atomic", title: "Paste at once"');
    expect(choices).toContain('{ id: "compatibility", title: "Type it"');
    expect(choices).not.toContain(">Atomic<");
    expect(choices).not.toContain(">Compatibility<");
    expect(choices).toContain("The encrypted text appears in one step.");
    expect(choices).toContain("types the same text one key at a time.");
  });

  it("shows the same comparison in onboarding and Settings", () => {
    const onboarding = functionSource("sendingSetupContent", "bindOnboarding");
    const settings = functionSource("sendingSettingsContent", "privacySettingsContent");
    expect(onboarding).toContain("placementOptionsMarkup(true)");
    expect(onboarding).toContain("placementComparisonMarkup()");
    expect(settings).toContain("placementOptionsMarkup(false)");
    expect(settings).toContain("placementComparisonMarkup()");
  });

  it("uses CSS-only motion and a reduced-motion final state", () => {
    expect(styles).toContain("@keyframes placement-copy");
    expect(styles).toContain("@keyframes placement-type");
    expect(styles).toMatch(/@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.placement-demo-typing \{ width: 17ch !important; \}/);
    expect(functionSource("placementComparisonMarkup", "parseTheme")).not.toMatch(/setTimeout|setInterval|requestAnimationFrame/);
  });
});
