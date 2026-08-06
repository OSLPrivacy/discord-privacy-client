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
  // The 2026-08-06 redesign moved the markup into one shared entry screen that
  // Sign in, Create account and Finish setup all render, so the welcome rules
  // are now split across the caller and that component.
  const entry = functionSource("entryScreenContent", "welcomeOnboardingContent");

  // Guards the deletion: the entry screen is mark, one action, one way out. The
  // old heading, explainer paragraph, divider and "add another identity in
  // Settings" footnote were removed by the redesign and must not come back.
  it("shows the bare entry screen and keeps the removed introduction copy out", () => {
    expect(entry).toContain('<section class="signin-card signin-lock-screen" aria-labelledby="route-heading">');
    expect(entry).toContain('<h1 id="route-heading" class="sr-only" tabindex="-1">${label}</h1>');
    expect(entry).toContain('<img class="signin-ghost-mark" src="${oslGhostMarkUrl}"');
    expect(entry).toContain('<button class="signin-unlock" data-onboarding="${route}" type="button">');
    expect(entry).toContain('<button class="signin-recovery" data-onboarding="import" type="button">Use recovery phrase</button>');

    // Code only: the function's own comment lists the copy that was removed in
    // order to explain why, so the "it stays removed" check must not read it.
    const shown = `${welcome}${entry}`.replace(/^\s*\/\/.*$/gmu, "");
    expect(shown).not.toContain("Protect the accounts you already use");
    expect(shown).not.toContain("messaging, social and email accounts you already have");
    expect(shown).not.toContain("private place");
    expect(shown).not.toContain("OSL communication is built in");
    expect(shown).not.toMatch(/add another identity/iu);
  });

  // Protects the rule: which action the entry screen offers is derived from
  // account state, never hardcoded -- a returning device signs in, a
  // half-finished setup finishes, and anything else creates an account.
  it("keeps account state as the source of the primary action", () => {
    expect(welcome).toContain('const partialIdentity = core.readiness.identityLoaded && core.readiness.bootstrapStatus === "setupRequired"');
    expect(welcome).toContain('const returning = core.readiness.bootstrapStatus === "passwordRequired" || core.readiness.passwordGateRequired');
    expect(welcome).toContain('if (returning) return entryScreenContent("Sign in", signinLockIcon(), "unlock");');
    expect(welcome).toContain('return entryScreenContent(partialIdentity ? "Finish setup" : "Create account", signinPlusIcon(), "create");');
    // The route that state picked is what the button actually carries.
    expect(entry).toContain('data-onboarding="${route}"');
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
