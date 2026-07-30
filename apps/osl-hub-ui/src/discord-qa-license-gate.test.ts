import { describe, expect, it } from "vitest";
import fs from "node:fs";

const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("Discord QA license gate", () => {
  it("compile-gates every Pro onboarding route to the sending setup step", () => {
    expect(source).toContain(
      'return discordQaShell && candidate === "pro" ? "sending" : candidate;',
    );
    expect(source).toContain(
      "onboardingRoute = onboardingRouteForBuild(onboardingRoute);",
    );
    expect(source).toContain(
      'return onboardingRouteForBuild(routes[current] ?? "welcome");',
    );
    expect(source).toContain(
      'pendingOnboardingRoute() ?? onboardingRouteForBuild("pro")',
    );
  });

  it("does not expose activation settings in the QA shell", () => {
    expect(source).toMatch(
      /function activationSettingsContent\(\): string \{\s+if \(discordQaShell\) return "";/u,
    );
  });

  it("retains the production Pro onboarding and activation UI", () => {
    expect(source).toContain("function proSetupContent(): string");
    expect(source).toContain(">Enter Pro code</h1>");
    expect(source).toContain(">Activate Pro</button>");
  });
});
