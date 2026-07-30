import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const autoScrubContractSource = readFileSync(new URL("./autoscrub-contract.ts", import.meta.url), "utf8");
const simpleSpec = readFileSync(new URL("../../../docs/design/osl-simple-spec.md", import.meta.url), "utf8");
const claimGate = readFileSync(new URL("../../../scripts/check-app-claims.mjs", import.meta.url), "utf8");

describe("public claim copy contract", () => {
  it("keeps send outcomes tri-state and refuses automatic retry on uncertainty", () => {
    expect(simpleSpec).toContain("sent, not sent, or delivery uncertain");
    expect(simpleSpec).toContain("never auto-retries it");
    expect(simpleSpec).toMatch(/never asks\s+the user to resend as if the first attempt certainly failed/);
  });

  it("keeps visible app copy away from banned public claim phrases", () => {
    for (const source of [mainSource, autoScrubContractSource]) {
      expect(source).not.toContain("Protect local storage");
      expect(source).not.toContain("End-to-end encrypted");
      expect(source).not.toContain("reviewed local list");
      expect(source).not.toContain("scanned, previewed, confirmed, executed, and checked");
      expect(source).not.toContain("checked locally, previewed, approved");
    }
    expect(mainSource).toContain("Review device storage");
    expect(mainSource).toContain("Protected OSL messages");
    expect(autoScrubContractSource).toContain("local list you approve");
  });

  it("keeps the Scrub mutation self-test anchored to one production copy site", () => {
    const scrubMarker = "<span class=\"privacy-local-mark\">FREE · THIS DEVICE ONLY</span><h2>Recommended action</h2><h3>Review an export</h3>";
    expect(mainSource.split(scrubMarker)).toHaveLength(2);
    expect(claimGate).toContain("const scrubMarker = \"<span class=\\\"privacy-local-mark\\\">FREE · THIS DEVICE ONLY</span><h2>Recommended action</h2><h3>Review an export</h3>\"");
  });
});
