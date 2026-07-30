import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const LEGACY_DURESS_PRODUCT_CLAIMS = [
  /\bduress password\b[\s\S]{0,180}\bunlocks? the app normally\b/iu,
  /\bduress password\b[\s\S]{0,220}\bsilently (?:burns?|deletes?|destroys?|strips?)\b/iu,
  /\ball keys are destroyed\b/iu,
  /\b(?:privacy|OPSEC) features\b[^.\n]{0,120}\b(?:are )?stripped\b/iu,
  /\brequires? full reinstall\b/iu,
  /\bfailed[- ]attempt\b[^.\n]{0,180}\b(?:auto(?:matically)?[- ]?)?(?:burn|wipe|duress)\b/iu,
] as const;

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing production source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing production source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

function rustSourcesBelow(directory: URL): string {
  const root = directory.pathname;
  const sources: string[] = [];
  const visit = (path: string): void => {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const child = join(path, entry.name);
      if (entry.isDirectory()) visit(child);
      else if (entry.isFile() && entry.name.endsWith(".rs")) {
        sources.push(readFileSync(child, "utf8"));
      }
    }
  };
  visit(root);
  return sources.join("\n");
}

describe("legacy duress wipe production reachability", () => {
  it("wire a distinct duress PIN check at the unlock screen that triggers Du", () => {
    const uiMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

    const unlockHandler = sourceBetween(
      uiMain,
      "function bindPasswordForm(): void {",
      "\nfunction bindImportForm(): void {",
    );
    const unlockBranch = sourceBetween(
      unlockHandler,
      "const gate = await unlockHubPasswordGate(secret);",
      "services = await loadLinkedServices().catch(() => services);",
    );
    const burnedBranch = sourceBetween(
      unlockBranch,
      'if (gate.outcome === "burned") {',
      "\n        }",
    );

    expect(unlockBranch).toContain("const gate = await unlockHubPasswordGate(secret);");
    expect(unlockBranch.indexOf('if (gate.outcome === "burned")')).toBeGreaterThan(
      unlockBranch.indexOf('if (gate.outcome === "decoy")'),
    );
    expect(unlockBranch.indexOf('if (gate.outcome === "burned")')).toBeLessThan(
      unlockBranch.indexOf('if (!gate.readiness?.unlocked)'),
    );
    expect(burnedBranch).toContain("localStorage.clear();");
    expect(burnedBranch).toContain("onboardingComplete = false;");
    expect(burnedBranch).toContain('core = structuredClone(unavailableCoreIntegration);');
    expect(burnedBranch).toContain('onboardingRoute = "welcome";');
    expect(burnedBranch).toContain("showToast(gate.burn?.localCleanupComplete");
    expect(burnedBranch).toContain("render();");
    expect(burnedBranch).toContain("return;");

    const mutation = unlockBranch.replace('if (gate.outcome === "burned") {', 'if (gate.outcome === "unlocked") {');
    expect(
      mutation.indexOf('if (gate.outcome === "burned")'),
      "mutation must remove the distinct burned outcome branch",
    ).toBe(-1);
  });

  it("does not sell the implemented-but-unwired DuressEngine sequence", () => {
    const keystoreDuress = readFileSync(
      new URL("../../../crates/keystore/src/duress.rs", import.meta.url),
      "utf8",
    );
    const keystoreLib = readFileSync(
      new URL("../../../crates/keystore/src/lib.rs", import.meta.url),
      "utf8",
    );
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
    const uiCore = readFileSync(new URL("./core.ts", import.meta.url), "utf8");
    const uiMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const onboarding = readFileSync(
      new URL("../../../docs/ONBOARDING.md", import.meta.url),
      "utf8",
    );
    const nativeProduction = rustSourcesBelow(
      new URL("../../osl-hub/src", import.meta.url),
    );
    const handler = sourceBetween(nativeMain, "tauri::generate_handler![", "\n    ]);");
    const publicProductClaims = `${onboarding}\n${uiMain}`;

    expect(keystoreLib).toContain("pub mod duress;");
    expect(keystoreLib).toContain("DuressEngine, DuressError, DuressHandlers");
    expect(keystoreDuress).toContain("pub struct DuressEngine");
    expect(keystoreDuress).toContain("pub fn execute(&self)");
    expect(keystoreDuress).toContain("pub fn resume_if_pending(&self)");
    expect(keystoreDuress).toContain("pub struct DuressHandlers");
    expect(keystoreDuress).toContain("wipe_local_cache_dir: Option<WipeFn>");
    expect(keystoreDuress).toContain("None => StepOutcome::Skipped");

    const legacyEngineImported = /\bDuressEngine\b/u.test(nativeProduction);
    const legacyEngineConstructed = /\bDuressEngine::new\s*\(/u.test(nativeProduction);
    const legacyEngineExecuted =
      /\bresume_if_pending\s*\(/u.test(nativeProduction)
      || /\bDuressEngine\b[\s\S]{0,800}\.execute\s*\(/u.test(nativeProduction);
    const legacyCommandRegistered = /\bduress\b/iu.test(handler);
    const legacyUiCaller =
      /\binvoke(?:<[^>]+>)?\(\s*["'][^"']*duress[^"']*["']/iu.test(uiCore)
      && /\bduress\b/iu.test(uiMain);
    const legacyProductionReachable =
      legacyEngineImported
      && legacyEngineConstructed
      && legacyEngineExecuted
      && legacyCommandRegistered
      && legacyUiCaller;

    expect(legacyEngineImported).toBe(false);
    expect(legacyEngineConstructed).toBe(false);
    expect(legacyEngineExecuted).toBe(false);
    expect(legacyCommandRegistered).toBe(false);
    expect(legacyUiCaller).toBe(false);
    expect(legacyProductionReachable).toBe(false);
    if (!legacyProductionReachable) {
      for (const claim of LEGACY_DURESS_PRODUCT_CLAIMS) {
        expect(publicProductClaims).not.toMatch(claim);
      }
    }

    const burnCommandRegistered = /\bunlock_hub_password_gate\b/u.test(handler);
    const burnRoleClassified = /VerifiedGateRole::Burn/u.test(startupGate);
    const burnDispatched = /cleanup::execute_verified_gate_burn\s*\(/u.test(nativeMain);
    const burnUiInvokes =
      /invoke<unknown>\(\s*["']unlock_hub_password_gate["']/u.test(uiCore);
    const burnUiHandles = /if \(gate\.outcome === ["']burned["']\)/u.test(uiMain);
    const currentBurnProductionReachable =
      burnCommandRegistered
      && burnRoleClassified
      && burnDispatched
      && burnUiInvokes
      && burnUiHandles;

    expect(burnCommandRegistered).toBe(true);
    expect(burnRoleClassified).toBe(true);
    expect(burnDispatched).toBe(true);
    expect(burnUiInvokes).toBe(true);
    expect(burnUiHandles).toBe(true);
    expect(currentBurnProductionReachable).toBe(true);
    expect(cleanup).toContain("pub fn execute_verified_gate_burn(");
    expect(uiMain).toContain(
      "Erases OSL data from this device when entered at sign in.",
    );
    expect(onboarding).toMatch(
      /The current desktop\s+app does not call that\s+`DuressEngine`\./u,
    );

    const mutations = [
      "A duress password unlocks the app normally while cleanup runs.",
      "A duress password silently destroys all encrypted messages.",
      "All keys are destroyed — there is no recovery.",
      "The app's privacy features are stripped.",
      "Restoring privacy requires full reinstall.",
      "A failed-attempt threshold automatically triggers the duress wipe.",
    ];
    for (const mutation of mutations) {
      expect(
        LEGACY_DURESS_PRODUCT_CLAIMS.some((claim) => claim.test(mutation)),
        `legacy duress claim escaped the mutation gate: ${mutation}`,
      ).toBe(true);
    }
  });
});
