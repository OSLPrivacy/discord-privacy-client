import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseCoreReadiness } from "./core";

/**
 * D-207 — the renderer's half.
 *
 * The native fix stops a device-bound key being minted over a password gate and
 * stops `unlocked` being satisfied by one. That is only half the product claim:
 * the renderer still has to ROUTE on the result. These are the two places it
 * can go wrong on this side — accepting a payload that contradicts itself, and
 * folding the lost-key state into a branch that offers the wrong action.
 */

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

const base = {
  originalCoreLinked: true,
  identityLoaded: true,
  keyserverInitialised: true,
  cloudRegistrationState: "registered",
  groupSenderKeysEnabled: true,
  remoteServiceHasNativeAccess: false,
  bootstrapAttempted: true,
  passwordGateRequired: true,
  unlocked: false,
  activeOslUserId: null,
  storageMethod: null,
};

describe("D-207 reopen routing", () => {
  it("carries identityKeyLost as its own status rather than collapsing it", () => {
    const parsed = parseCoreReadiness({
      ...base,
      identityLoaded: false,
      bootstrapStatus: "identityKeyLost",
    });
    expect(parsed.originalCoreLinked).toBe(true);
    expect(parsed.bootstrapStatus).toBe("identityKeyLost");
    expect(parsed.unlocked).toBe(false);
  });

  it("rejects a payload claiming the key is lost AND the identity loaded", () => {
    // Incoherent: `identityKeyLost` means the blob would not open. Accepting it
    // would let the renderer act on a state the native side cannot produce.
    const parsed = parseCoreReadiness({
      ...base,
      identityLoaded: true,
      bootstrapStatus: "identityKeyLost",
    });
    expect(parsed.originalCoreLinked).toBe(false);
  });

  it("still rejects passwordRequired claiming to be unlocked", () => {
    // The pre-existing contract D-207 slipped past on the NATIVE side. Kept
    // asserted here so the two halves cannot drift apart.
    const parsed = parseCoreReadiness({
      ...base,
      bootstrapStatus: "passwordRequired",
      unlocked: true,
    });
    expect(parsed.originalCoreLinked).toBe(false);
  });

  it("routes a lost device key above both setup and unlock", () => {
    const keyLost = source.indexOf('core.readiness.bootstrapStatus === "identityKeyLost"');
    const setup = source.indexOf('core.readiness.bootstrapStatus === "setupRequired"', keyLost);
    const password = source.indexOf('core.readiness.bootstrapStatus === "passwordRequired"', keyLost);
    expect(keyLost, "bootstrap must branch on identityKeyLost").toBeGreaterThan(0);
    expect(keyLost).toBeLessThan(setup);
    expect(keyLost).toBeLessThan(password);
    expect(source).toContain('onboardingRoute = "keylost"');
    expect(source).toContain('if (onboardingRoute === "keylost") return identityKeyLostContent();');
  });

  it("offers the recovery phrase and no password box on the lost-key screen", () => {
    const start = source.indexOf("function identityKeyLostContent");
    const end = source.indexOf("function welcomeOnboardingContent", start);
    expect(start).toBeGreaterThan(0);
    const screen = source.slice(start, end);
    expect(screen).toContain("This device can no longer open your account");
    expect(screen).toContain("recovery phrase");
    expect(screen).toContain('data-onboarding="import"');
    // The failure this screen exists to stop being mistaken for.
    expect(screen).not.toContain('data-onboarding="unlock"');
    expect(screen).not.toContain('data-onboarding="create"');
    expect(screen).not.toContain("type=\"password\"");
  });

  it("does not report a lost-key account as unlocked anywhere in the UI", () => {
    const start = source.indexOf("function accountUnlocked");
    const end = source.indexOf("function passwordSecuritySettingsContent", start);
    expect(start).toBeGreaterThan(0);
    expect(source.slice(start, end)).toContain('!== "identityKeyLost"');
  });
});
