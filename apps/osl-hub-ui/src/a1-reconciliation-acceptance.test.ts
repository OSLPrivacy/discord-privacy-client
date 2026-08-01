import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createHubOslIdentity,
  identityProtectionStatus,
  importHubOslIdentityPhrase,
  setupHubMainPassword,
} from "./core";
import {
  recoveryKitReducer,
  recoveryKitView,
  type RecoveryKitSecrets,
  visibleRecoverySecrets,
} from "./recovery-kit";
import { RecoveryCaptureGate } from "./ui-behavior";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));

const identityPhrase = "abandon ability able about above absent absorb abstract absurd abuse access accident";
const passwordPhrase = "account advice aerobic affair agent ahead aim alarm album alert alien alley";

const readyReadiness = {
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

function recoveryState(secrets: RecoveryKitSecrets, captureProven: boolean) {
  return {
    secrets,
    captureProven,
    captureEnforcement: "enforced" as const,
    shownWithoutProtection: false,
    savedAcknowledged: false,
    kitUnsaved: true,
  };
}

describe("A1 reconciliation acceptance", () => {
  beforeEach(() => {
    native.invoke.mockReset();
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
  });

  it("creates a device-protected identity, reloads encrypted state, and holds its recovery kit until capture protection is proven", async () => {
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "create_hub_osl_identity") {
        return {
          userId: "osl_1234567890abcdef1234567890abcdef12345678",
          identityRecoveryPhrase: identityPhrase,
          storageMethod: "keyring",
          passwordSetupRequired: true,
        };
      }
      if (command === "setup_hub_main_password") {
        return {
          passwordRecoveryPhrase: passwordPhrase,
          encryptedStateReloadComplete: true,
          encryptedStateReloadIssueCount: 0,
          readiness: readyReadiness,
        };
      }
      throw new Error(`unexpected native command: ${command}`);
    });

    const identity = await createHubOslIdentity();
    const password = await setupHubMainPassword("correct-horse-battery-staple");

    expect(native.invoke.mock.calls.map(([command]) => command)).toEqual([
      "create_hub_osl_identity",
      "setup_hub_main_password",
    ]);
    expect(native.invoke.mock.calls[0][1]).toEqual({
      ownerAuthorizationSignoff: {
        ownerPresent: true,
        reviewedNoExistingIdentityReplacement: true,
        acceptsRecoveryPhraseResponsibility: true,
      },
    });
    expect(identityProtectionStatus(identity.storageMethod).state).toBe("protected");
    expect(password).toMatchObject({
      encryptedStateReloadComplete: true,
      encryptedStateReloadIssueCount: 0,
      readiness: { unlocked: true },
    });

    const secrets = {
      userId: identity.userId,
      identityPhrase: identity.identityRecoveryPhrase,
      passwordPhrase: password.passwordRecoveryPhrase,
    };
    const gate = new RecoveryCaptureGate();
    expect(visibleRecoverySecrets(recoveryState(secrets, gate.canRender()))).toBeNull();

    expect(gate.accept(gate.checkpoint())).toBe(true);
    expect(visibleRecoverySecrets(recoveryState(secrets, gate.canRender()))).toEqual(secrets);
  });

  it("imports a normalized recovery phrase, then publishes only the newly-created password recovery phrase", async () => {
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "import_hub_osl_identity_phrase") {
        return {
          userId: "osl_abcdef1234567890abcdef1234567890abcdef12",
          identityRecoveryPhrase: identityPhrase,
          storageMethod: "tpm-pcp",
          passwordSetupRequired: true,
        };
      }
      if (command === "setup_hub_main_password") {
        return {
          passwordRecoveryPhrase: passwordPhrase,
          encryptedStateReloadComplete: true,
          encryptedStateReloadIssueCount: 0,
          readiness: readyReadiness,
        };
      }
      throw new Error(`unexpected native command: ${command}`);
    });

    const identity = await importHubOslIdentityPhrase(`  ${identityPhrase}  `);
    const password = await setupHubMainPassword("correct-horse-battery-staple");
    const secrets = {
      userId: identity.userId,
      identityPhrase: null,
      passwordPhrase: password.passwordRecoveryPhrase,
    };

    expect(native.invoke.mock.calls.map(([command]) => command)).toEqual([
      "import_hub_osl_identity_phrase",
      "setup_hub_main_password",
    ]);
    expect(native.invoke.mock.calls[0][1]).toEqual({ recoveryPhrase: identityPhrase });
    expect(identityProtectionStatus(identity.storageMethod).state).toBe("protected");
    expect(visibleRecoverySecrets(recoveryState(secrets, true))).toEqual(secrets);
    expect(visibleRecoverySecrets(recoveryState(secrets, true))?.identityPhrase).toBeNull();
  });

  it("withdraws recovery secrets immediately when the accepted capture proof becomes stale", () => {
    const secrets = { userId: "osl-local", identityPhrase, passwordPhrase };
    const gate = new RecoveryCaptureGate();
    expect(gate.accept(gate.checkpoint())).toBe(true);
    expect(recoveryKitView(recoveryState(secrets, gate.canRender())).mode).toBe("kit");

    gate.invalidate();
    const heldBack = recoveryState(secrets, gate.canRender());
    expect(recoveryKitView(heldBack).mode).toBe("refusal");
    expect(visibleRecoverySecrets(heldBack)).toBeNull();

    const deferred = recoveryKitReducer(heldBack, { kind: "remind-me-later" });
    expect(deferred.outcome).toBe("leave-recovery");
    expect(deferred.state).toMatchObject({ secrets: null, kitUnsaved: true });
  });
});
