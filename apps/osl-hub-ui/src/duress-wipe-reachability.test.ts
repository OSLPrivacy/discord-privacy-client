import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing production source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing production source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("duress wipe unlock reachability", () => {
  it("routes the unlock-screen burn password through the typed gate and destructive branch", () => {
    const uiMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const uiCore = readFileSync(new URL("./core.ts", import.meta.url), "utf8");
    const ipcState = readFileSync(
      new URL("../../../crates/ipc/src/state.rs", import.meta.url),
      "utf8",
    );
    const keystoreDuress = readFileSync(
      new URL("../../../crates/keystore/src/duress.rs", import.meta.url),
      "utf8",
    );

    const unlockSubmit = sourceBetween(
      uiMain,
      "function bindPasswordForm(): void {",
      "\nfunction bindImportForm",
    );
    const burnBranch = sourceBetween(
      unlockSubmit,
      "if (isVerifiedBurnGate(gate)) {",
      'if (!gate.readiness?.unlocked)',
    );
    const unlockForm = sourceBetween(
      uiMain,
      "function identityPasswordForm(",
      "\nfunction isVerifiedBurnGate",
    );

    expect(uiCore).toContain('invoke<unknown>("unlock_hub_password_gate", { password })');
    expect(unlockSubmit).toContain("const gate = await unlockHubPasswordGate(secret)");
    expect(uiMain).toContain('return gate.outcome === "burned";');
    expect(burnBranch).toContain("localStorage.clear()");
    expect(burnBranch).toContain("core = structuredClone(unavailableCoreIntegration)");
    expect(burnBranch).toContain('onboardingRoute = "welcome"');
    expect(burnBranch).not.toMatch(/loadLinkedServices|loadCoreIntegration|fetch\(|invoke\(/u);

    expect(ipcState).toContain("pub duress_engine: Mutex<keystore::DuressEngine>");
    expect(ipcState).toContain("keystore::build_production_duress_engine");
    expect(keystoreDuress).toContain("pub fn build_production_duress_engine");
    expect(keystoreDuress).toContain("pub fn execute(&self)");

    expect(unlockForm).not.toMatch(/DuressEngine|keyserver|ratchet|provider adapter|browser profile/iu);

    const mutations = [
      uiCore.replaceAll("unlock_hub_password_gate", "unlock_hub_main_password"),
      unlockSubmit.replaceAll("const gate = await unlockHubPasswordGate(secret)", "await loadCoreIntegration()"),
      uiMain.replaceAll('return gate.outcome === "burned";', "return false;"),
      burnBranch.replaceAll("localStorage.clear()", ""),
      ipcState.replaceAll("pub duress_engine: Mutex<keystore::DuressEngine>", ""),
    ];
    expect(mutations[0]).not.toContain('invoke<unknown>("unlock_hub_password_gate", { password })');
    expect(mutations[1]).not.toContain("const gate = await unlockHubPasswordGate(secret)");
    expect(mutations[2]).not.toContain('return gate.outcome === "burned";');
    expect(mutations[3]).not.toContain("localStorage.clear()");
    expect(mutations[4]).not.toContain("pub duress_engine: Mutex<keystore::DuressEngine>");
  });
});
