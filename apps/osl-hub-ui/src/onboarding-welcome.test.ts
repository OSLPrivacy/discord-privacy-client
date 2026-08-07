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

describe("onboarding welcome content", () => {
  const welcome = functionSource("welcomeOnboardingContent", "proSetupContent");
  const entry = functionSource("entryScreenContent", "welcomeOnboardingContent");

  it("shows the Welcome choice screen and keeps removed introduction copy out", () => {
    expect(welcome).toContain('<section class="signin-card signin-lock-screen welcome-choice-screen" aria-labelledby="route-heading">');
    expect(welcome).toContain('<h1 id="route-heading" tabindex="-1">Welcome</h1>');
    expect(welcome).toContain('<button class="signin-unlock" data-onboarding="create" type="button"><span class="signin-unlock-label">Create</span>');
    expect(welcome).toContain('<button class="signin-unlock" data-onboarding="import" type="button"><span class="signin-unlock-label">Restore</span>');
    expect(welcome).toContain('<button class="signin-unlock" data-onboarding="unlock" type="button"><span class="signin-unlock-label">Unlock</span>');

    // Code only: the function's own comment lists the copy that was removed in
    // order to explain why, so the "it stays removed" check must not read it.
    const shown = `${welcome}${entry}`.replace(/^\s*\/\/.*$/gmu, "");
    expect(shown).not.toContain("Protect the accounts you already use");
    expect(shown).not.toContain("messaging, social and email accounts you already have");
    expect(shown).not.toContain("private place");
    expect(shown).not.toContain("OSL communication is built in");
    expect(shown).not.toMatch(/add another identity/iu);
  });

  it("keeps the shared entry screen available for direct create and unlock routes", () => {
    expect(entry).toContain('<h1 id="route-heading" class="sr-only" tabindex="-1">${label}</h1>');
    expect(entry).toContain('<button class="signin-unlock" data-onboarding="${route}" type="button">');
    expect(entry).toContain('<button class="signin-recovery" data-onboarding="import" type="button">Use recovery phrase</button>');
  });

  // Protects the first-run reading level. The copy lives in the shared entry
  // screen now, so the rule has to read that too or it checks nothing.
  it("does not expose implementation concepts in the welcome copy", () => {
    expect(`${welcome}${entry}`).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter/i);
  });

  it("is the only welcome branch rendered by onboardingContent", () => {
    const content = functionSource("onboardingContent", "welcomeOnboardingContent");
    expect(content).toContain('if (onboardingRoute === "welcome") return welcomeOnboardingContent();');
  });
});
