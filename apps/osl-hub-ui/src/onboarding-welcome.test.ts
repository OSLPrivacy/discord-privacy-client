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

  it("introduces OSL around existing accounts and private OSL communication", () => {
    expect(welcome).toContain("Protect the accounts you already use");
    expect(welcome).toContain("messaging, social and email accounts you already have");
    expect(welcome).toContain("private place");
    expect(welcome).toContain("OSL communication is built in");
  });

  it("keeps account state as the source of the primary action", () => {
    expect(welcome).toContain('const partialIdentity = core.readiness.identityLoaded && core.readiness.bootstrapStatus === "setupRequired"');
    expect(welcome).toContain('const returning = core.readiness.bootstrapStatus === "passwordRequired" || core.readiness.passwordGateRequired');
    expect(welcome).toContain('const primaryRoute: OnboardingRoute = partialIdentity ? "create" : returning ? "unlock" : "create"');
    expect(welcome).toContain('partialIdentity ? "Finish setup"');
    expect(welcome).toContain('returning ? "Unlock this device"');
    expect(welcome).toContain('data-onboarding="${primaryRoute}"');
  });

  it("does not expose implementation concepts in the welcome copy", () => {
    expect(welcome).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter/i);
  });

  it("is the only welcome branch rendered by onboardingContent", () => {
    const content = functionSource("onboardingContent", "welcomeOnboardingContent");
    expect(content).toContain('if (onboardingRoute === "welcome") return welcomeOnboardingContent();');
  });
});
