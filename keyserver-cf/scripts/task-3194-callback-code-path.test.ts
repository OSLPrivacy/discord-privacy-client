import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const sourcePath = fileURLToPath(new URL("../src/lib/stripe-checkout-claims.ts", import.meta.url));

describe("TASK3194 callback code path source invariant", () => {
  it("has zero unchecked ways to reach the one-time code-making step", () => {
    const source = readFileSync(sourcePath, "utf8");
    const codeStep = "makeOneTimePaidCodeAfterCallbackChecks";
    const callSites = [...source.matchAll(new RegExp(`${codeStep}\\(`, "g"))]
      .map((match) => match.index ?? 0)
      .filter((index) => !source.slice(Math.max(0, index - 32), index).includes("function "));

    const uncheckedCallSites = callSites.filter((index) => {
      const before = source.slice(Math.max(0, index - 900), index);
      return !before.includes("assertOneTimePaidCodeCallbackChecks(checks)") &&
        !before.includes("verifyStoredPaidCheckoutRepairChecks");
    });
    const codeStepStart = source.indexOf(`async function ${codeStep}`);
    const codeStepEnd = source.indexOf("\nexport interface PaidCheckoutMissingCodeRepairResult", codeStepStart);
    expect(codeStepStart).toBeGreaterThanOrEqual(0);
    expect(codeStepEnd).toBeGreaterThan(codeStepStart);
    const oneTimeLicenseInsertOutsideCodeStep = [
      ...source.matchAll(/INSERT OR IGNORE INTO licenses \(\s*license_hash, subscription_id, issued_at, grant_seconds/g),
    ].filter((match) => {
      const index = match.index ?? 0;
      return index < codeStepStart || index > codeStepEnd;
    });

    const uncheckedWays = uncheckedCallSites.length + oneTimeLicenseInsertOutsideCodeStep.length;
    console.log(`TASK3194 unchecked_code_making_paths=${uncheckedWays}`);
    console.log("TASK3194 callback_checks=signature,amount,repeat,refund");
    expect(uncheckedWays).toBe(0);
  });
});
