import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const LEGACY_DURESS_PRODUCT_CLAIMS = [
  /\bduress password\b[\s\S]{0,180}\bunlocks? the app normally\b/iu,
  /\bduress password\b[\s\S]{0,220}\bsilently (?:burns?|deletes?|destroys?|strips?)\b/iu,
  /\ball keys are destroyed\b/iu,
  /\b(?:privacy|OPSEC) features\b[^.\n]{0,120}\b(?:are )?stripped\b/iu,
  /\brequires? full reinstall\b/iu,
] as const;

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing production source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing production source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("duress wipe production reachability", () => {
  it("routes the unlock-screen burn password through the typed gate and destructive branch", () => {
    const uiCore = readFileSync(new URL("./core.ts", import.meta.url), "utf8");
    const uiMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const nativeMain = readFileSync(
      new URL("../../osl-hub/src/main.rs", import.meta.url),
      "utf8",
    );
    const startupGate = readFileSync(
      new URL("../../osl-hub/src/startup_gate.rs", import.meta.url),
      "utf8",
    );
    const cleanup = readFileSync(
      new URL("../../osl-hub/src/cleanup.rs", import.meta.url),
      "utf8",
    );
    const ipcState = readFileSync(
      new URL("../../../crates/ipc/src/state.rs", import.meta.url),
      "utf8",
    );
    const keystoreDuress = readFileSync(
      new URL("../../../crates/keystore/src/duress.rs", import.meta.url),
      "utf8",
    );

    const unlockForm = sourceBetween(
      uiMain,
      'data-password-mode="unlock"',
      "function onboardingPasswordRoleContent",
    );
    const passwordBinding = sourceBetween(
      uiMain,
      "function bindPasswordForm(): void",
      "function bindImportForm(): void",
    );
    const burnBranch = sourceBetween(
      passwordBinding,
      "if (unlockScreenDuressPinTriggeredWipe(gate)) {",
      "if (!gate.readiness?.unlocked)",
    );

    expect(keystoreDuress).toContain("pub struct DuressEngine");
    expect(keystoreDuress).toContain("pub fn build_production_duress_handlers");
    expect(keystoreDuress).toContain("pub fn build_production_duress_engine");
    expect(keystoreDuress).toContain("pub fn execute(&self)");
    expect(ipcState).toContain("pub duress_engine: Mutex<keystore::DuressEngine>");
    expect(ipcState).toContain("new_with_production_duress_engine");
    expect(ipcState).toContain("execute_production_duress");

    expect(nativeMain).toContain("async fn unlock_hub_password_gate(");
    expect(nativeMain).toContain("unlock_hub_password_gate,");
    expect(startupGate).toContain("VerifiedGateRole::Burn");
    expect(startupGate).toContain("VerifiedGateRole::Duress");
    expect(startupGate).not.toContain("role_after_auto_burn_threshold");
    expect(nativeMain).toContain("cleanup::execute_verified_gate_burn");
    expect(cleanup).toContain("pub fn execute_verified_gate_burn(");

    expect(uiCore).toContain('invoke<unknown>("unlock_hub_password_gate", { password })');
    expect(uiCore).toContain('"unlocked", "decoy", "burned", "duress", "wrong"');
    expect(uiCore).toContain('(["burned", "duress"].includes(raw.outcome as string)) !== (burn !== null)');

    expect(unlockForm).toContain('id="identity-password-form"');
    expect(unlockForm).toContain('id="identity-password"');
    expect(unlockForm).not.toMatch(/DuressEngine|keyserver|ratchet|receipt|provider adapter|browser profile/iu);
    expect(passwordBinding).toContain("const gate = await checkUnlockScreenCredential(secret);");
    expect(passwordBinding).toContain("return unlockHubPasswordGate(secret);");
    expect(passwordBinding).toContain("function unlockScreenDuressPinTriggeredWipe");
    expect(passwordBinding).toContain('return (gate.outcome === "burned" || gate.outcome === "duress") && gate.burn !== null;');
    expect(passwordBinding).toContain("if (unlockScreenDuressPinTriggeredWipe(gate))");
    expect(burnBranch).toContain("localStorage.clear();");
    expect(burnBranch).toContain("core = structuredClone(unavailableCoreIntegration);");
    expect(burnBranch).toContain('onboardingRoute = "welcome";');
    expect(burnBranch).toContain("gate.burn?.localCleanupComplete");
    expect(burnBranch).not.toMatch(/loadLinkedServices|loadCoreIntegration|fetch\(|invoke\(/u);

    for (const claim of LEGACY_DURESS_PRODUCT_CLAIMS) {
      expect(uiMain).not.toMatch(claim);
    }

    const forbiddenMutations = [
      'const gate = await unlockHubPasswordGate(secret);',
      'return gate.outcome === "burned";',
      'return gate.outcome === "duress";',
      'if (gate.outcome === "burned") {',
      'if (gate.outcome === "duress") {',
    ];
    for (const mutation of forbiddenMutations) {
      expect(
        passwordBinding.includes(mutation) || unlockForm.includes(mutation),
        `mutation should not already be present: ${mutation}`,
      ).toBe(false);
    }
  });
});
