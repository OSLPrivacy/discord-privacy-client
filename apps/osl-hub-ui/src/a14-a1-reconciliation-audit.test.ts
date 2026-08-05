import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { identityProtectionStatus, parseIdentitySetupResult, parseMainPasswordSetupResult } from "./core";
import { initialRecoveryKitState, recoveryKitReducer, recoveryKitView, visibleRecoverySecrets } from "./recovery-kit";

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of every `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of each test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// tests read it synchronously. That is safe here and was checked, not assumed:
// the one test that touches the module drives its state entirely through
// `__oslHubUiTest.reset(...)`, and the stubbed `localStorage` is emptied before
// each test -- which is exactly the state a fresh import would have seen.
const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  localStore.clear();
});

describe("A1 reconciliation independent audit (unit a14)", () => {
  it("holds recovery secrets back until capture protection is proven or the owner explicitly accepts the risk", () => {
    const secrets = { userId: "osl-user", identityPhrase: "alpha beta", passwordPhrase: "gamma delta" };
    const initial = initialRecoveryKitState(secrets, false);

    expect(recoveryKitView(initial)).toMatchObject({ mode: "refusal", secretsVisible: false });
    expect(visibleRecoverySecrets(initial)).toBeNull();

    const protectedState = recoveryKitReducer(initial, {
      kind: "protection-proved",
      proven: true,
      enforcement: "enforced",
    }).state;
    expect(visibleRecoverySecrets(protectedState)).toEqual(secrets);
    expect(recoveryKitReducer(protectedState, { kind: "continue" }).outcome).toBe("rejected");

    const unprotectedState = recoveryKitReducer(initial, { kind: "show-anyway", acknowledgement: "show anyway" }).state;
    expect(visibleRecoverySecrets(unprotectedState)).toEqual(secrets);
    const completed = recoveryKitReducer(
      recoveryKitReducer(unprotectedState, { kind: "set-saved-acknowledged", acknowledged: true }).state,
      { kind: "continue" },
    );
    expect(completed).toMatchObject({ outcome: "leave-recovery", state: { secrets: null, kitUnsaved: false } });
  });

  it("renders verified device protection as ready and treats unknown or unverified methods as not secure", () => {
    expect(identityProtectionStatus("tpm-pcp")).toMatchObject({ state: "protected", label: "Account protected" });
    for (const method of [null, undefined, "memory-ephemeral", "future-device-vault"]) {
      expect(identityProtectionStatus(method)).toMatchObject({ state: "not-secure", label: "Account not secure" });
    }

    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ route: "settings", coreReady: true, storageMethod: "tpm-pcp" });
    expect(__oslHubUiTest.renderRouteShell("settings")).toContain('data-identity-protection="protected"');

    __oslHubUiTest.reset({ route: "settings", coreReady: true, storageMethod: "future-device-vault" });
    expect(__oslHubUiTest.renderRouteShell("settings")).toContain('data-identity-protection="not-secure"');
  });

  it("accepts only complete identity and password-setup results required to resume encrypted state", () => {
    expect(parseIdentitySetupResult({
      userId: "osl-user",
      identityRecoveryPhrase: "alpha beta",
      storageMethod: "keyring",
      passwordSetupRequired: true,
    })).toMatchObject({ userId: "osl-user", storageMethod: "keyring" });
    expect(() => parseIdentitySetupResult({
      userId: "osl-user",
      identityRecoveryPhrase: null,
      passwordSetupRequired: true,
    })).toThrow("invalid identity setup response");

    const passwordSetup = {
      passwordRecoveryPhrase: "gamma delta",
      encryptedStateReloadComplete: true,
      encryptedStateReloadIssueCount: 0,
      readiness: {},
    };
    expect(parseMainPasswordSetupResult(passwordSetup)).toEqual(passwordSetup);
    expect(() => parseMainPasswordSetupResult({ ...passwordSetup, encryptedStateReloadIssueCount: -1 })).toThrow("invalid password setup response");
    expect(() => parseMainPasswordSetupResult({ ...passwordSetup, encryptedStateReloadComplete: undefined })).toThrow("invalid password setup response");
  });
});
