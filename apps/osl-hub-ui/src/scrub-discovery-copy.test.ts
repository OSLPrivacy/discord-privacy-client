import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * v1 Scrub is DISCOVERY: it shows the owner what an export exposes and gives
 * manual directions. It deletes nothing.
 *
 * The two surfaces wired in for that -- grouped owner-review rows and the scope
 * digest -- are the ones most easily misread as a deletion product: a row with
 * a count looks like a queue, and a hex digest looks like a receipt. Neither is.
 * These checks hold the disclaimers that say so to the shipping markup, because
 * the reachability test only proves the modules are imported, not that what they
 * render is honest about what the build did.
 */

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, next: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${next}`, start);
  expect(start).toBeGreaterThanOrEqual(0);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("Scrub discovery surfaces state what the build does", () => {
  it("renders grouped review rows and the scope digest from the scan results", () => {
    const results = functionSource("privacyScanResultsMarkup", "scrubFindingLabel");
    expect(results).toContain("buildScrubReviewList(");
    expect(results).toContain("scrubScopeFingerprintMarkup()");
  });

  it("says the grouped rows are a second view that deletes nothing", () => {
    const rows = functionSource("scrubReviewRowsMarkup", "bytesToBase64");
    expect(rows).toContain("this build deletes nothing");
    expect(rows).toContain("counted once");
    // The grouped view must not become a second selection surface; choosing
    // what to review stays in the one list the review dialog reads.
    expect(rows).not.toContain("selectedScrubFindings");
    expect(rows).not.toContain("<input");
  });

  it("says the scope digest describes what was reviewed, not what was deleted", () => {
    const digest = functionSource("scrubScopeFingerprintMarkup", "scrubSignalGroupLabel");
    expect(digest).toContain("It is not proof that anything was deleted.");
    expect(digest).toContain("It changes when you change the categories.");
  });

  it("fingerprints the scope the findings were actually stamped with", () => {
    const input = functionSource("scrubScopeFingerprintInput", "refreshScrubScopeFingerprint");
    expect(input).toContain("serviceId: localScrubScanServiceId");
    expect(input).toContain("accountId: localScrubScanAccountId");
    // The same identifiers the local import writes onto every candidate, so the
    // digest can never describe a scope other than the one that was scanned.
    const scan = functionSource("scanPrivacyExport", "scrubScopeFingerprintInput");
    expect(scan).toContain("serviceId: localScrubScanServiceId");
    expect(scan).toContain("accountId: localScrubScanAccountId");
  });

  it("shows no digest at all when one cannot be computed", () => {
    const refresh = functionSource("refreshScrubScopeFingerprint", "scrubScopeFingerprintMarkup");
    expect(refresh).toContain("scrubScopeFingerprint = null");
    const markup = functionSource("scrubScopeFingerprintMarkup", "scrubSignalGroupLabel");
    expect(markup).toContain("if (!scrubScopeFingerprint) return \"\";");
  });

  it("clears the digest with the scan it describes", () => {
    const clear = functionSource("clearPrivacyScanState", "privacyScanResultsMarkup");
    expect(clear).toContain("scrubScopeFingerprint = null");
    expect(source).toContain('document.querySelector<HTMLButtonElement>("#clear-privacy-scan")?.addEventListener("click", () => { clearPrivacyScanState(); render(); });');
  });
});
