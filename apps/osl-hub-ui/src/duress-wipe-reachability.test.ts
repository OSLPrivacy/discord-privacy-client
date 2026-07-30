import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const CLAIMS_THAT_MUST_NOT_SURFACE = [
  /\bduress password\b[\s\S]{0,180}\bunlocks? the app normally\b/iu,
  /\bduress password\b[\s\S]{0,220}\bsilently (?:burns?|deletes?|destroys?|strips?)\b/iu,
  /\ball keys are destroyed\b/iu,
  /\b(?:privacy|OPSEC) features\b[^.\n]{0,120}\b(?:are )?stripped\b/iu,
  /\brequires? full reinstall\b/iu,
] as const;

function readRelative(path: string): string {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing end marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("duress wipe production reachability", () => {
  it("wire a distinct duress PIN check at the unlock screen that triggers DuressEngine", () => {
    const main = readRelative("./main.ts");
    const core = readRelative("./core.ts");
    const state = readRelative("../../../crates/ipc/src/state.rs");
    const keystoreDuress = readRelative("../../../crates/keystore/src/duress.rs");

    expect(keystoreDuress).toContain("pub struct DuressEngine");
    expect(keystoreDuress).toContain("pub fn build_production_duress_handlers(");
    expect(state).toContain("pub duress_engine: Mutex<keystore::DuressEngine>");
    expect(state).toContain("default_production_duress_engine()");
    expect(core).toContain('"unlocked", "decoy", "duress", "burned", "wrong"');

    const duressBranch = sourceBetween(
      main,
      'if (gate.outcome === "duress") {',
      'if (gate.outcome === "burned") {',
    );

    expect(duressBranch).toContain("localStorage.clear()");
    expect(duressBranch).toContain("setup = parseSetupState(null)");
    expect(duressBranch).toContain("structuredClone(unavailableCoreIntegration)");
    expect(duressBranch).toContain('onboardingRoute = "welcome"');
    expect(duressBranch).not.toMatch(/\bgate\.outcome === "burned"\b/u);
    expect(duressBranch).not.toMatch(/\bgate\.burn\b/u);
    expect(duressBranch).not.toMatch(/\bfetch\s*\(/u);
    expect(duressBranch).not.toMatch(/\binvoke\s*\(/u);
    expect(duressBranch).not.toMatch(/discord|platform|recipient/iu);

    for (const claim of CLAIMS_THAT_MUST_NOT_SURFACE) {
      expect(main).not.toMatch(claim);
    }
  });
});
