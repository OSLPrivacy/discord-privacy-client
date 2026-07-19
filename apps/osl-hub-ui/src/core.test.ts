import { describe, expect, it } from "vitest";
import { coreReadinessLabel, isActivationCode, isCoreProtectionReady, isRecoveryPhrase, isValidMainPassword, isValidNewMainPassword, loadCoreIntegrationFromNative, parseCoreFeatures, parseCoreReadiness, parseHubLicenseState, parseHubPasswordRoleStatus, parseIdentitySetupResult, parseMainPasswordSetupResult } from "./core";

describe("original OSL core bridge", () => {
  it("accepts the exact non-sensitive readiness contract", () => {
    const parsed = parseCoreReadiness({
      originalCoreLinked: true,
      identityLoaded: false,
      keyserverInitialised: false,
      groupSenderKeysEnabled: false,
      remoteServiceHasNativeAccess: false,
    });
    expect(parsed.originalCoreLinked).toBe(true);
    expect(parsed.unlocked).toBe(false);
    expect(coreReadinessLabel(parsed)).toBe("source linked · bootstrap required");
  });

  it("fails closed when readiness grows an unexpected authority field", () => {
    expect(parseCoreReadiness({
      originalCoreLinked: true,
      identityLoaded: true,
      keyserverInitialised: true,
      cloudRegistrationState: "registered",
      groupSenderKeysEnabled: true,
      remoteServiceHasNativeAccess: true,
      allowRemoteInvoke: true,
    }).originalCoreLinked).toBe(false);
  });

  it("accepts a unique bounded feature manifest and rejects unsafe ids", () => {
    expect(parseCoreFeatures([{ id: "scope-burn", group: "Retention", label: "Scope burn", bridgeState: "source-linked" }])).toHaveLength(1);
    expect(parseCoreFeatures([{ id: "../scope", group: "Retention", label: "Scope burn", bridgeState: "core-linked" }])).toEqual([]);
  });

  it("accepts the extended readiness contract only when locally unlocked", () => {
    const parsed = parseCoreReadiness({
      originalCoreLinked: true,
      identityLoaded: true,
      keyserverInitialised: true,
      cloudRegistrationState: "registered",
      groupSenderKeysEnabled: false,
      remoteServiceHasNativeAccess: false,
      bootstrapAttempted: true,
      passwordGateRequired: true,
      unlocked: true,
      activeOslUserId: "osl-user-preview",
      bootstrapStatus: "ready",
    });
    expect(isCoreProtectionReady(parsed)).toBe(true);
    expect(coreReadinessLabel(parsed)).toBe("OSL core ready · locally unlocked");
  });

  it("recognizes an isolated identity setup requirement", () => {
    const parsed = parseCoreReadiness({
      originalCoreLinked: true,
      identityLoaded: false,
      keyserverInitialised: false,
      cloudRegistrationState: "notAttempted",
      groupSenderKeysEnabled: false,
      remoteServiceHasNativeAccess: false,
      bootstrapAttempted: true,
      passwordGateRequired: false,
      unlocked: false,
      activeOslUserId: null,
      bootstrapStatus: "setupRequired",
    });
    expect(parsed.bootstrapStatus).toBe("setupRequired");
    expect(isCoreProtectionReady(parsed)).toBe(false);
  });

  it("does not request the nonessential feature manifest during fresh setup", async () => {
    const invoked: string[] = [];
    const integration = await loadCoreIntegrationFromNative(async (command) => {
      invoked.push(command);
      expect(command).toBe("get_core_readiness");
      return {
        originalCoreLinked: true,
        identityLoaded: false,
        keyserverInitialised: false,
        cloudRegistrationState: "notAttempted",
        groupSenderKeysEnabled: false,
        remoteServiceHasNativeAccess: false,
        bootstrapAttempted: true,
        passwordGateRequired: false,
        unlocked: true,
        activeOslUserId: null,
        bootstrapStatus: "setupRequired",
      };
    });
    expect(integration.readiness.bootstrapStatus).toBe("setupRequired");
    expect(integration.readiness.originalCoreLinked).toBe(true);
    expect(integration.features).toEqual([]);
    expect(invoked).toEqual(["get_core_readiness"]);
  });

  it("keeps malformed boot readiness fail closed", async () => {
    const integration = await loadCoreIntegrationFromNative(async () => ({
      originalCoreLinked: true,
      identityLoaded: false,
      keyserverInitialised: false,
      groupSenderKeysEnabled: false,
      remoteServiceHasNativeAccess: false,
      bootstrapAttempted: true,
      passwordGateRequired: false,
      unlocked: true,
      activeOslUserId: null,
      bootstrapStatus: "setupRequired",
      unexpectedAuthority: true,
    }));

    expect(integration.readiness.originalCoreLinked).toBe(false);
    expect(integration.readiness.bootstrapStatus).toBe("notAttempted");
  });

  it("fails closed on partial extensions, unknown status, or impossible unlock state", () => {
    const base = {
      originalCoreLinked: true,
      identityLoaded: true,
      keyserverInitialised: true,
      cloudRegistrationState: "registered",
      groupSenderKeysEnabled: false,
      remoteServiceHasNativeAccess: false,
    };
    expect(parseCoreReadiness({ ...base, bootstrapAttempted: true }).originalCoreLinked).toBe(false);
    expect(parseCoreReadiness({
      ...base,
      bootstrapAttempted: true,
      passwordGateRequired: false,
      unlocked: false,
      activeOslUserId: null,
      bootstrapStatus: "complete-ish",
    }).originalCoreLinked).toBe(false);
    expect(parseCoreReadiness({
      ...base,
      identityLoaded: false,
      bootstrapAttempted: true,
      passwordGateRequired: true,
      unlocked: true,
      activeOslUserId: "user",
      bootstrapStatus: "ready",
    }).originalCoreLinked).toBe(false);
  });

  it("never reports protection ready from a local client without confirmed Cloudflare registration", () => {
    const parsed = parseCoreReadiness({
      originalCoreLinked: true,
      identityLoaded: true,
      keyserverInitialised: true,
      cloudRegistrationState: "offline",
      groupSenderKeysEnabled: false,
      remoteServiceHasNativeAccess: false,
      bootstrapAttempted: true,
      passwordGateRequired: true,
      unlocked: true,
      activeOslUserId: "osl-user-preview",
      bootstrapStatus: "failed",
    });
    expect(isCoreProtectionReady(parsed)).toBe(false);
    expect(coreReadinessLabel(parsed)).toBe("OSL cloud is unavailable");
  });

  it("accepts six-character passwords while recommending longer ones in the UI", () => {
    expect(isValidMainPassword("abc123")).toBe(true);
    expect(isValidMainPassword("seven77")).toBe(true);
    expect(isValidMainPassword("mediumlen")).toBe(true);
    expect(isValidMainPassword("elevenchars")).toBe(true);
    expect(isValidMainPassword("correct horse battery staple")).toBe(true);
    expect(isValidMainPassword("short")).toBe(false);
    expect(isValidMainPassword("abc12\n")).toBe(false);
    expect(isValidMainPassword("päss12")).toBe(false);
    expect(isValidNewMainPassword("abc123")).toBe(true);
    expect(isValidNewMainPassword("short")).toBe(false);
    expect(isValidNewMainPassword("twelve-chars!")).toBe(true);
  });

  it("validates recovery phrases and one-time identity setup results", () => {
    expect(isRecoveryPhrase("abandon ability able about above absent absorb abstract absurd abuse access accident")).toBe(true);
    expect(isRecoveryPhrase("only eleven words are not enough for this local recovery phrase")).toBe(false);
    expect(parseIdentitySetupResult({
      userId: "osl_1234567890abcdef1234567890abcdef12345678",
      identityRecoveryPhrase: "abandon ability able about above absent absorb abstract absurd abuse access accident",
      storageMethod: "os-keyring",
      passwordSetupRequired: true,
    }).passwordSetupRequired).toBe(true);
  });

  it("rejects authority growth in password setup responses", () => {
    const readiness = {
      accessState: "ready",
      identityLoaded: true,
      mainPasswordSet: true,
      unlocked: true,
      serviceNeutralIdentitySupported: true,
      canCreateIdentity: false,
      canImportIdentityPhrase: false,
      passwordAttemptsUsed: 0,
      passwordLockoutSecondsRemaining: 0,
    };
    expect(parseMainPasswordSetupResult({
      passwordRecoveryPhrase: "recovery words",
      encryptedStateReloadComplete: true,
      encryptedStateReloadIssueCount: 0,
      readiness,
    }).readiness.accessState).toBe("ready");
    expect(() => parseMainPasswordSetupResult({
      passwordRecoveryPhrase: "recovery words",
      encryptedStateReloadComplete: true,
      encryptedStateReloadIssueCount: 0,
      readiness,
      remoteToken: "no",
    })).toThrow();
  });

  it("validates browser-delivered activation codes and exact native state", () => {
    expect(isActivationCode(" osl-ab12-cd34-ef56-gh78 ")).toBe(true);
    expect(isActivationCode("OSL-AB12-CD34-EF56-GH7!")).toBe(false);
    expect(parseHubLicenseState({
      access: "pro",
      status: "ACTIVE",
      currentPeriodEnd: 1_800_000_000,
      lastValidatedAt: 1_700_000_000,
    }).access).toBe("pro");
  });

  it("fails closed on inconsistent or expanded activation state", () => {
    expect(() => parseHubLicenseState({
      access: "pro",
      status: "EXPIRED",
      currentPeriodEnd: null,
      lastValidatedAt: null,
    })).toThrow();
    expect(() => parseHubLicenseState({
      access: "free",
      status: "UNCONFIGURED",
      currentPeriodEnd: null,
      lastValidatedAt: null,
      remoteCheckoutToken: "no",
    })).toThrow();
  });

  it("keeps configured alternate passwords distinct from enabled login actions", () => {
    const parsed = parseHubPasswordRoleStatus({
      mainPasswordSet: true,
      stealthPasswordSet: true,
      burnPasswordSet: false,
      unlocked: true,
      stealthActionWired: false,
      burnActionWired: false,
    });
    expect(parsed.stealthPasswordSet).toBe(true);
    expect(parsed.stealthActionWired).toBe(false);
    expect(() => parseHubPasswordRoleStatus({ ...parsed, remoteAction: true })).toThrow();
  });
});
