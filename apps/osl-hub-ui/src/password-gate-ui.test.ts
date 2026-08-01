import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const core = readFileSync(new URL("./core.ts", import.meta.url), "utf8");

describe("password gate UI", () => {
  it("routes every unlock through the typed role gate", () => {
    expect(source).toContain("await checkUnlockScreenCredential(secret)");
    expect(source).not.toContain("unlockHubMainPassword");
    expect(core).not.toContain('invoke<unknown>("unlock_hub_main_password"');
  });

  it("loads no protected workspace data for stealth", () => {
    const start = source.indexOf('if (gate.outcome === "decoy")');
    const end = source.indexOf('if (gate.outcome === "duress")', start);
    const branch = source.slice(start, end);
    expect(branch).toContain("structuredClone(unavailableCoreIntegration)");
    expect(branch).toContain("services = []");
    expect(branch).toContain('onboardingRoute = "decoy"');
    expect(branch).not.toMatch(/loadLinkedServices|listHubPeople|loadFriendProfile/);
  });

  // A7: the "Lock now" button is the only manual path to the real session
  // lock. A renamed data attribute or a dropped bindWorkspace line would leave
  // a button that renders and does nothing, which is worse than no button.
  it("renders a Lock now button and binds it to the real lock command", () => {
    expect(source).toContain('data-lock-session="now"');
    expect(source).toContain('querySelectorAll<HTMLButtonElement>("[data-lock-session]")');
    expect(source).toContain("void lockSessionNow(button)");
    expect(source).toContain("await lockHubSession()");
    expect(core).toContain('invoke<unknown>("lock_hub_session")');
    // It belongs on the unlocked branch only: offering "Lock now" while the
    // gate is already up would be a no-op button.
    const start = source.indexOf("function passwordSecuritySettingsContent()");
    const unlockedBranch = source.slice(start, source.indexOf("const roleForm", start));
    expect(unlockedBranch).toContain("Password configured and unlocked");
    expect(unlockedBranch).toContain('data-lock-session="now"');
  });

  it("clears only OSL UI state after a verified burn result", () => {
    const start = source.indexOf("if (isVerifiedBurnGate(gate))");
    const end = source.indexOf('if (!gate.readiness?.unlocked)', start);
    const branch = source.slice(start, end);
    expect(branch).toContain("localStorage.clear()");
    expect(branch).toContain("setup = parseSetupState(null)");
    expect(branch).toContain('onboardingRoute = "welcome"');
    expect(branch).not.toMatch(/fetch\(|invoke\(|removeItem\([^)]*discord/i);
  });
});
