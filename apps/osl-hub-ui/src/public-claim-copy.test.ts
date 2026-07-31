import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const autoScrubContractSource = readFileSync(new URL("./autoscrub-contract.ts", import.meta.url), "utf8");
const simpleSpec = readFileSync(new URL("../../../docs/design/osl-simple-spec.md", import.meta.url), "utf8");
const claimGate = readFileSync(new URL("../../../scripts/check-app-claims.mjs", import.meta.url), "utf8");

type BurnGuaranteeClass =
  | "local_cleanup"
  | "osl_server_cleanup"
  | "cooperative_peer_request"
  | "exact_scope"
  | "honest_result";

interface ParsedBurnContract {
  guarantees: Array<{ title: string; body: string; guarantee: BurnGuaranteeClass }>;
  bannedClaims: string[];
}

function markdownSection(source: string, heading: string): string {
  const lines = source.split("\n");
  const start = lines.findIndex((line) => line.trim() === heading);
  expect(start, `missing ${heading}`).toBeGreaterThanOrEqual(0);
  const level = heading.match(/^#+/u)?.[0].length ?? 1;
  const end = lines.findIndex((line, index) => {
    const match = /^(#+)\s/u.exec(line);
    return index > start && match !== null && match[1]!.length <= level;
  });
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n");
}

function classifyBurnGuarantee(body: string): BurnGuaranteeClass {
  if (
    /local stored ciphertext/u.test(body) &&
    /cached attachments/u.test(body) &&
    /sync cannot resurrect/u.test(body)
  ) {
    return "local_cleanup";
  }
  if (/server-side state/u.test(body) && /selected OSL scope/u.test(body)) {
    return "osl_server_cleanup";
  }
  if (
    /peers to clean up/u.test(body) &&
    /consent, binding, authority/u.test(body) &&
    /refused or reported as unavailable/u.test(body)
  ) {
    return "cooperative_peer_request";
  }
  if (/reviewed and confirmed scope/u.test(body) && /requires confirmation again/u.test(body)) {
    return "exact_scope";
  }
  if (
    /actually verified/u.test(body) &&
    /never displayed as deletion/u.test(body) &&
    /unsupported or unverified/u.test(body)
  ) {
    return "honest_result";
  }
  throw new Error(`unclassified Burn guarantee: ${body}`);
}

function parseBurnContract(source: string): ParsedBurnContract {
  const burn = markdownSection(source, "## Burn");
  const guaranteeMatches = [...burn.matchAll(
    /^\d+\.\s+\*\*(?<title>[^*]+):\*\*\s+(?<body>.*(?:\n {3}.+)*)$/gmu,
  )];
  const guarantees = guaranteeMatches.map((match) => {
    const title = match.groups?.title ?? "";
    const body = (match.groups?.body ?? "").replace(/\s+/gu, " ").trim();
    return { title, body, guarantee: classifyBurnGuarantee(body) };
  });
  const bannedClaims = [...burn.matchAll(/"([^"]+)"/gu)].map((match) => match[1]!);
  return { guarantees, bannedClaims };
}

function rejectsPublicBurnClaim(contract: ParsedBurnContract, claim: string): boolean {
  const normalized = claim.toLowerCase();
  return contract.bannedClaims.some((banned) => normalized.includes(banned.toLowerCase())) ||
    /delete carrier messages|erase provider retention|erase backups|erase screenshots|un-send/u.test(
      normalized,
    );
}

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

  it("docs/design/osl-simple-spec.md'", () => {
    const burn = parseBurnContract(simpleSpec);

    expect(burn.guarantees.map((item) => item.guarantee)).toEqual([
      "local_cleanup",
      "osl_server_cleanup",
      "cooperative_peer_request",
      "exact_scope",
      "honest_result",
    ]);
    expect(new Set(burn.guarantees.map((item) => item.title)).size).toBe(5);
    expect(burn.bannedClaims).toHaveLength(8);

    for (const banned of burn.bannedClaims) {
      expect(rejectsPublicBurnClaim(burn, `Burn makes content ${banned}.`)).toBe(true);
    }
    expect(rejectsPublicBurnClaim(
      burn,
      "Burn can delete carrier messages and erase backups.",
    )).toBe(true);
    expect(rejectsPublicBurnClaim(
      burn,
      "Burn cleans up local OSL data and reports unverified peer cleanup as unavailable.",
    )).toBe(false);
  });

  it("keeps the Scrub mutation self-test anchored to one production copy site", () => {
    const scrubMarker = "<span class=\"privacy-local-mark\">FREE · THIS DEVICE ONLY</span><h2>Recommended action</h2><h3>Review an export</h3>";
    expect(mainSource.split(scrubMarker)).toHaveLength(2);
    expect(claimGate).toContain("const scrubMarker = \"<span class=\\\"privacy-local-mark\\\">FREE · THIS DEVICE ONLY</span><h2>Recommended action</h2><h3>Review an export</h3>\"");
  });
});
