import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const renderer = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");
const core = readFileSync(fileURLToPath(new URL("./core.ts", import.meta.url)), "utf8");
const passwordLifecycle = readFileSync(
  fileURLToPath(new URL("../../osl-hub/src/password_lifecycle.rs", import.meta.url)),
  "utf8",
);
const identityStorage = readFileSync(
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

function matchingLines(source: string, needle: string): string[] {
  return source
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.includes(needle));
}

function recoveryObjectAssignments(source: string): string[] {
  return matchingLines(source, "recoveryBundle =").filter((line) => !line.includes("= null"));
}

function assertProtectedRecoveryPublication(flow: string): void {
  expectOrdered(flow, "recoveryBundle =", "recoverySavedAcknowledged = false;");
  expectOrdered(flow, "recoverySavedAcknowledged = false;", "onboardingRoute = \"recovery\";");
  expectOrdered(flow, "onboardingRoute = \"recovery\";", "await proveRecoveryCaptureProtection();");

  const publicationIndex = flow.indexOf("recoveryBundle =");
  const proofIndex = flow.indexOf("await proveRecoveryCaptureProtection();");
  const firstRender = flow.indexOf("render()", publicationIndex);
  if (firstRender >= 0) {
    expect(proofIndex, "recovery secrets must be capture-proven before the first render after publication")
      .toBeLessThan(firstRender);
  }
}

describe("A1 reconciliation independent audit (unit a14)", () => {
  it("accounts for every recovery-secret publication path and requires capture proof before render", () => {
    expect(matchingLines(renderer, "recoveryBundle =")).toEqual([
      "recoveryBundle = null;",
      "recoveryBundle = {",
      "recoveryBundle = { userId: identity.userId, identityPhrase: null, passwordPhrase: passwordResult.passwordRecoveryPhrase };",
      "recoveryBundle = null;",
    ]);
    expect(recoveryObjectAssignments(renderer)).toHaveLength(2);

    const createFlow = sourceBetween(
      sourceBetween(renderer, "function bindPasswordForm", "function bindImportForm"),
      "if (setupMode) {\n        const identity",
      "      } else {\n        const gate",
    );
    expectOrdered(createFlow, "identityStorageMethod = identity.storageMethod", "recoveryBundle = {");
    expectOrdered(createFlow, "await setupHubMainPassword(secret)", "recoveryBundle = {");
    assertProtectedRecoveryPublication(createFlow);

    const importFlow = sourceBetween(renderer, "function bindImportForm", "function renderWorkspace");
    expectOrdered(importFlow, "identityStorageMethod = identity.storageMethod", "recoveryBundle = {");
    expectOrdered(importFlow, "await setupHubMainPassword(passwordSecret)", "recoveryBundle = {");
    assertProtectedRecoveryPublication(importFlow);
  });

  it("keeps the at-rest identity claim tied to verified method labels and fail-closed unknowns", () => {
    expect(matchingLines(renderer, "identityStorageMethod =")).toEqual([
      "if (identity) identityStorageMethod = identity.storageMethod;",
      "identityStorageMethod = null;",
      "identityStorageMethod = null;",
      "identityStorageMethod = null;",
      "identityStorageMethod = identity.storageMethod;",
      "identityStorageMethod = knownIdentityStorageMethods.get(slotId) ?? null;",
      "identityStorageMethod = null;",
    ]);
    expect(matchingLines(renderer, "knownIdentityStorageMethods.set")).toEqual([
      "knownIdentityStorageMethods.set(created.identity.slotId, created.storageMethod);",
      "knownIdentityStorageMethods.set(recovered.identity.slotId, recovered.storageMethod);",
    ]);

    const accountSettings = sourceBetween(
      renderer,
      "function identitySettingsContent(): string {",
      "\nfunction activationSettingsContent(",
    );
    expect(accountSettings).toContain("identityStorageProtectionMarkup(classifyIdentityStorageProtection(identityStorageMethod))");
    expect(accountSettings).not.toContain("identityStorageProtectionMarkup(classifyIdentityStorageProtection(core.readiness.storageMethod))");

    const protectionStatus = sourceBetween(
      core,
      "export function identityProtectionStatus(storageMethod: string | null | undefined): IdentityProtectionStatus {",
      "\nexport function parseMainPasswordSetupResult",
    );
    expect(core).toContain('const protectedStorageMethods = new Set(["tpm-pcp", "keyring", "os-keyring"]);');
    expect(protectionStatus).toContain("if (storageMethod && protectedStorageMethods.has(storageMethod))");
    expect(protectionStatus).toContain('state: "protected"');
    expect(protectionStatus).toContain('state: "not-secure"');
    expect(protectionStatus.indexOf('state: "protected"')).toBeLessThan(protectionStatus.indexOf('state: "not-secure"'));
  });

  it("checks the backend facts the reconciliation depends on are still present", () => {
    expect(core).toContain('["userId", "identityRecoveryPhrase", "storageMethod", "passwordSetupRequired"]');
    expect(core).toContain('["passwordRecoveryPhrase", "encryptedStateReloadComplete", "encryptedStateReloadIssueCount", "readiness"]');

    const persistentSealer = sourceBetween(
      passwordLifecycle,
      "pub(crate) fn persistent_sealer()",
      "fn ensure_empty_identity_slot",
    );
    expect(persistentSealer).toContain("keystore::sealer::METHOD_TPM | keystore::sealer::METHOD_KEYRING => Ok(sealer)");
    expect(persistentSealer).not.toContain("METHOD_EPHEMERAL => Ok");
    expect(persistentSealer).not.toContain("METHOD_NOOP => Ok");

    const passwordSetupHelper = sourceBetween(
      passwordLifecycle,
      "fn setup_main_password_using",
      "fn isolated_account_dir",
    );
    expect(passwordSetupHelper).toContain("reload_encrypted_state_after_unlock(&state.osl, account_dir)");
    expect(passwordLifecycle).toContain("encrypted_state_reload_complete: outcome.reload_issue_count == 0");
    expect(identityStorage).toContain("sealed_b64: STANDARD.encode(&sealed)");
    expect(identityStorage).toContain("method: sealer.method_label().to_string()");
  });
});
