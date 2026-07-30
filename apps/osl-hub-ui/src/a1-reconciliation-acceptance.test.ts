import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const renderer = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");
const core = readFileSync(fileURLToPath(new URL("./core.ts", import.meta.url)), "utf8");
const passwordLifecycle = readFileSync(
  fileURLToPath(new URL("../../osl-hub/src/password_lifecycle.rs", import.meta.url)),
  "utf8",
);
const storage = readFileSync(
  fileURLToPath(new URL("../../../crates/keystore/src/storage.rs", import.meta.url)),
  "utf8",
);

function sourceBetween(source: string, start: string, end: string): string {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from + start.length);
  expect(from, `missing source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(to, `missing end marker after ${start}: ${end}`).toBeGreaterThan(from);
  return source.slice(from, to);
}

function expectOrdered(source: string, earlier: string, later: string): void {
  const earlierIndex = source.indexOf(earlier);
  const laterIndex = source.indexOf(later);
  expect(earlierIndex, `missing earlier marker: ${earlier}`).toBeGreaterThanOrEqual(0);
  expect(laterIndex, `missing later marker: ${later}`).toBeGreaterThanOrEqual(0);
  expect(earlierIndex, `${earlier} must precede ${later}`).toBeLessThan(laterIndex);
}

function expectCaptureProofBeforeRecoveryPublication(flow: string): void {
  expectOrdered(flow, "recoveryBundle =", "await proveRecoveryCaptureProtection();");
  expect(flow).toContain("onboardingRoute = \"recovery\";");
  const bundleIndex = flow.indexOf("recoveryBundle =");
  const proofIndex = flow.indexOf("await proveRecoveryCaptureProtection();");
  const firstRenderAfterBundle = flow.indexOf("render()", bundleIndex);
  if (firstRenderAfterBundle >= 0) {
    expect(proofIndex, "capture proof must happen before recovery render").toBeLessThan(firstRenderAfterBundle);
  }
}

function expectDeviceSealedIdentityContract(): void {
  const persistentSealer = sourceBetween(
    passwordLifecycle,
    "pub(crate) fn persistent_sealer()",
    "fn ensure_empty_identity_slot",
  );
  expect(persistentSealer).toContain("keystore::sealer::METHOD_TPM | keystore::sealer::METHOD_KEYRING => Ok(sealer)");
  expect(persistentSealer).not.toContain("METHOD_EPHEMERAL => Ok");
  expect(persistentSealer).not.toContain("METHOD_NOOP => Ok");

  const install = sourceBetween(passwordLifecycle, "fn install_identity", "pub(crate) fn native_user_id");
  expectOrdered(install, "keystore::save_identity(&path, &identity, sealer)", "storage_method: sealer.method_label().to_owned()");
  expect(storage).toContain("sealed_b64: STANDARD.encode(&sealed)");
  expect(storage).toContain("method: sealer.method_label().to_string()");
}

describe("A1 reconciliation acceptance", () => {
  it("ties fresh account creation to device sealing, encrypted reload, and protected recovery publication", () => {
    const bindPassword = sourceBetween(renderer, "function bindPasswordForm", "function bindImportForm");
    const createFlow = sourceBetween(
      bindPassword,
      "if (setupMode) {\n        const identity",
      "      } else {\n        const gate",
    );
    const passwordSetup = sourceBetween(
      passwordLifecycle,
      "pub fn setup_main_password",
      "struct PasswordSetupOutcome",
    );
    const passwordSetupHelper = sourceBetween(
      passwordLifecycle,
      "fn setup_main_password_using",
      "fn isolated_account_dir",
    );

    expectOrdered(createFlow, "await createHubOslIdentity()", "await setupHubMainPassword(secret)");
    expectOrdered(createFlow, "await setupHubMainPassword(secret)", "core = await loadCoreIntegration()");
    expectOrdered(createFlow, "core = await loadCoreIntegration()", "recoveryBundle = {");
    expect(createFlow).toContain("identity?.userId ?? core.readiness.activeOslUserId");
    expect(createFlow).toContain("identity?.identityRecoveryPhrase ?? null");
    expect(createFlow).toContain("passwordResult.passwordRecoveryPhrase");
    expectCaptureProofBeforeRecoveryPublication(createFlow);

    expect(core).toContain('["userId", "identityRecoveryPhrase", "storageMethod", "passwordSetupRequired"]');
    expect(core).toContain('["passwordRecoveryPhrase", "encryptedStateReloadComplete", "encryptedStateReloadIssueCount", "readiness"]');
    expect(passwordSetup).toContain("encrypted_state_reload_complete: outcome.reload_issue_count == 0");
    expect(passwordSetupHelper).toContain("reload_encrypted_state_after_unlock(&state.osl, account_dir)");
    expectDeviceSealedIdentityContract();
  });

  it("ties recovery-phrase import to the same sealed local identity and protected recovery publication", () => {
    const importFlow = sourceBetween(renderer, "function bindImportForm", "function renderWorkspace");
    const importNative = sourceBetween(
      passwordLifecycle,
      "pub fn import_native_identity_phrase",
      "pub fn setup_main_password",
    );

    expect(core).toContain("return parseIdentitySetupResult(await invoke<unknown>(\"import_hub_osl_identity_phrase\", { recoveryPhrase: recoveryPhrase.trim() }));");
    expect(importNative).toContain("parse_identity_phrase(&phrase)?");
    expect(importNative).toContain("keystore::identity_from_entropy(entropy, \"osl-pending\".to_owned())");
    expect(importNative).toContain("identity.user_id = native_user_id(&identity);");
    expect(importNative).toContain("persistent_sealer()?");
    expect(importNative).toContain("None,\n        !current.main_password_set");

    expectOrdered(importFlow, "await importHubOslIdentityPhrase(phraseSecret)", "phraseSecret = \"\";");
    expectOrdered(importFlow, "phraseSecret = \"\";", "await setupHubMainPassword(passwordSecret)");
    expect(importFlow).toContain("recoveryBundle = { userId: identity.userId, identityPhrase: null, passwordPhrase: passwordResult.passwordRecoveryPhrase }");
    expectCaptureProofBeforeRecoveryPublication(importFlow);
    expectDeviceSealedIdentityContract();
  });

  it("would fail this aggregate if recovery publication lost its capture proof", () => {
    const bindPassword = sourceBetween(renderer, "function bindPasswordForm", "function bindImportForm");
    const createFlow = sourceBetween(
      bindPassword,
      "if (setupMode) {\n        const identity",
      "      } else {\n        const gate",
    );
    const mutated = createFlow.replace("        await proveRecoveryCaptureProtection();", "");

    expect(() => expectCaptureProofBeforeRecoveryPublication(mutated)).toThrow();
  });
});
