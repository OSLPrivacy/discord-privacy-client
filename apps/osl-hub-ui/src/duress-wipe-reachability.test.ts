import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const CLAIMS_THAT_MUST_NOT_SURFACE = [
  /\bduress password\b[\s\S]{0,180}\bunlocks? the app normally\b/iu,
  /\bduress password\b[\s\S]{0,220}\bsilently (?:burns?|deletes?|destroys?|strips?)\b/iu,
  /\ball keys are destroyed\b/iu,
  /\b(?:privacy|OPSEC) features\b[^.\n]{0,120}\b(?:are )?stripped\b/iu,
  /\brequires? full reinstall\b/iu,
  /\bfailed[- ]attempt\b[^.\n]{0,180}\b(?:auto(?:matically)?[- ]?)?(?:burn|wipe|duress)\b/iu,
] as const;

function readRelative(path: string): string {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing production source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing production source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("duress wipe production reachability", () => {
  it("wires a distinct duress outcome at the unlock screen", () => {
    const main = readRelative("./main.ts");
    const core = readRelative("./core.ts");
    const state = readRelative("../../../crates/ipc/src/state.rs");
    const keystoreDuress = readRelative("../../../crates/keystore/src/duress.rs");
    const onboarding = readRelative("../../../docs/ONBOARDING.md");
    const publicProductClaims = `${onboarding}\n${main}`;

    expect(keystoreDuress).toContain("pub struct DuressEngine");
    expect(keystoreDuress).toContain("pub fn build_production_duress_handlers");
    expect(state).toContain("pub duress_engine: Mutex<keystore::DuressEngine>");
    expect(state).toContain("default_production_duress_engine()");
    expect(core).toContain('"unlocked", "decoy", "burned", "duress", "wrong"');
    expect(core).toContain('(raw.outcome === "burned") !== (burn !== null)');

    const duressBranch = sourceBetween(
      main,
      'if (gate.outcome === "duress") {',
      "if (isVerifiedBurnGate(gate)) {",
    );

    expect(duressBranch).toContain("localStorage.clear()");
    expect(duressBranch).toContain("setup = parseSetupState(null)");
    expect(duressBranch).toContain("structuredClone(unavailableCoreIntegration)");
    expect(duressBranch).toContain('onboardingRoute = "welcome"');
    // D80: silent, for the same reason as the burn branch below.
    expect(duressBranch).not.toMatch(/showToast/u);
    expect(duressBranch).not.toMatch(/\bgate\.outcome === "burned"\b/u);
    expect(duressBranch).not.toMatch(/\bgate\.burn\b/u);
    expect(duressBranch).not.toMatch(/\bfetch\s*\(/u);
    expect(duressBranch).not.toMatch(/\binvoke\s*\(/u);
    expect(duressBranch).not.toMatch(/discord|platform|recipient/iu);

    for (const claim of CLAIMS_THAT_MUST_NOT_SURFACE) {
      expect(publicProductClaims).not.toMatch(claim);
    }
  });

  it("wires a distinct burn outcome at the unlock screen", () => {
    const main = readRelative("./main.ts");
    const unlockHandler = sourceBetween(
      main,
      "function bindPasswordForm(): void {",
      "\nfunction bindImportForm(): void {",
    );
    const unlockBranch = sourceBetween(
      unlockHandler,
      "const gate = await checkUnlockScreenCredential(secret);",
      "core = await loadCoreIntegration();",
    );
    const burnedBranch = sourceBetween(
      unlockBranch,
      "if (isVerifiedBurnGate(gate)) {",
      "\n        }",
    );

    expect(unlockBranch.indexOf("if (isVerifiedBurnGate(gate))")).toBeGreaterThan(
      unlockBranch.indexOf('if (gate.outcome === "decoy")'),
    );
    expect(unlockBranch.indexOf("if (isVerifiedBurnGate(gate))")).toBeLessThan(
      unlockBranch.indexOf("if (!gate.readiness?.unlocked)"),
    );
    expect(burnedBranch).toContain("localStorage.clear();");
    expect(burnedBranch).toContain("onboardingComplete = false;");
    expect(burnedBranch).toContain("structuredClone(unavailableCoreIntegration)");
    expect(burnedBranch).toContain('onboardingRoute = "welcome"');
    expect(burnedBranch).toContain("render();");
    expect(burnedBranch).toContain("return;");
    // D80: the burn branch used to end with
    // `showToast("Verified local OSL cleanup completed")`. A toast announcing
    // the cleanup is the loudest possible tell -- it is printed on the screen
    // the person who took the device is holding. The burn and duress outcomes
    // must land on the welcome screen silently, so that they are
    // indistinguishable from a device that was simply never set up.
    expect(burnedBranch).not.toMatch(/showToast/u);

    const mutation = unlockBranch.replace(
      "if (isVerifiedBurnGate(gate)) {",
      'if (gate.outcome === "unlocked") {',
    );
    expect(
      mutation.indexOf("if (isVerifiedBurnGate(gate))"),
      "mutation must remove the distinct burned outcome branch",
    ).toBe(-1);
  });

  // D80 rewrote this test. It used to prove the burn code was reachable by
  // asserting the SEPARATE `#identity-duress-pin` input and the separate
  // `verify_duress_pin` verifier existed. Both are now security defects, so the
  // same property -- "the burn code still reaches the production duress engine"
  // -- is asserted through the single unified credential path instead.
  it("wires the burn code to the production duress engine through the single unlock input", () => {
    const nativeMain = readRelative("../../osl-hub/src/main.rs");
    const startupGate = readRelative("../../osl-hub/src/startup_gate.rs");
    const uiCore = readRelative("./core.ts");
    const uiMain = readRelative("./main.ts");

    // One input, one call, one argument.
    expect(uiMain).not.toContain("identity-duress-pin");
    expect(uiMain).not.toContain("data-duress-pin");
    expect(uiMain).toContain("return unlockHubPasswordGate(secret);");
    expect(uiMain).toContain("isVerifiedBurnGate(gate)");
    expect(uiCore).toContain("export async function unlockHubPasswordGate(password: string)");
    expect(uiCore).toContain('invoke<unknown>("unlock_hub_password_gate", { password })');
    expect(uiCore).not.toMatch(/duressPin/u);

    // One verifier on the native side, and it is the constant-time one that
    // compares all four role hashes rather than testing for a burn code first.
    expect(nativeMain).not.toMatch(/duress_pin/u);
    expect(nativeMain).not.toMatch(/verify_duress_pin/u);
    expect(nativeMain).toContain("startup_gate::verify_password_role(&verify_app.state::<HubCoreState>(), password)");
    expect(nativeMain).toContain(".duress_engine");
    expect(nativeMain).toContain(".execute()");
    expect(startupGate).not.toMatch(/pub fn verify_duress_pin\(/u);
    expect(startupGate).toContain("pub fn verify_password_role(");
    expect(startupGate).toContain('"burn" => VerifiedGateRole::Burn');
    expect(startupGate).toContain('"duress" => VerifiedGateRole::Duress');
  });
});
