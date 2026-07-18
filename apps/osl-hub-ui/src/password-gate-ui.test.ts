import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const core = readFileSync(new URL("./core.ts", import.meta.url), "utf8");

describe("password gate UI", () => {
  it("routes every unlock through the typed role gate", () => {
    expect(source).toContain("await unlockHubPasswordGate(secret)");
    expect(source).not.toContain("unlockHubMainPassword");
    expect(core).not.toContain('invoke<unknown>("unlock_hub_main_password"');
  });

  it("loads no protected workspace data for stealth", () => {
    const start = source.indexOf('if (gate.outcome === "decoy")');
    const end = source.indexOf('if (gate.outcome === "burned")', start);
    const branch = source.slice(start, end);
    expect(branch).toContain("structuredClone(unavailableCoreIntegration)");
    expect(branch).toContain("services = []");
    expect(branch).toContain('onboardingRoute = "decoy"');
    expect(branch).not.toMatch(/loadLinkedServices|listHubPeople|loadFriendProfile/);
  });

  it("clears only OSL UI state after a verified burn result", () => {
    const start = source.indexOf('if (gate.outcome === "burned")');
    const end = source.indexOf('if (!gate.readiness?.unlocked)', start);
    const branch = source.slice(start, end);
    expect(branch).toContain("localStorage.clear()");
    expect(branch).toContain("setup = parseSetupState(null)");
    expect(branch).toContain('onboardingRoute = "welcome"');
    expect(branch).not.toMatch(/fetch\(|invoke\(|removeItem\([^)]*discord/i);
  });
});
