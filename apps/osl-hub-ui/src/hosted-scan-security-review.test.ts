import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type HostedScanReviewRow = {
  check_id: string;
  boundary: string;
  authority_source: string;
  renderer_supplied: "yes" | "no";
  deletion_authority: "yes" | "no";
  refusal_without_authority: "refuse" | "permit";
};

const expectedRows: readonly HostedScanReviewRow[] = [
  {
    check_id: "hosted_identity_unlock",
    boundary: "unlocked OSL owner identity",
    authority_source: "native_state",
    renderer_supplied: "no",
    deletion_authority: "no",
    refusal_without_authority: "refuse",
  },
  {
    check_id: "hosted_active_context",
    boundary: "current service host generation",
    authority_source: "native_state",
    renderer_supplied: "no",
    deletion_authority: "no",
    refusal_without_authority: "refuse",
  },
  {
    check_id: "hosted_scope_binding",
    boundary: "active hosted scope binding",
    authority_source: "native_state",
    renderer_supplied: "no",
    deletion_authority: "no",
    refusal_without_authority: "refuse",
  },
  {
    check_id: "hosted_operator_binding",
    boundary: "attended operator-name binding",
    authority_source: "native_state",
    renderer_supplied: "no",
    deletion_authority: "no",
    refusal_without_authority: "refuse",
  },
  {
    check_id: "hosted_credential_handling",
    boundary: "hosted credential and profile material",
    authority_source: "unavailable_to_command",
    renderer_supplied: "no",
    deletion_authority: "no",
    refusal_without_authority: "refuse",
  },
  {
    check_id: "hosted_delete_boundary",
    boundary: "delete-own-item authority",
    authority_source: "not_present_in_scan_port",
    renderer_supplied: "no",
    deletion_authority: "no",
    refusal_without_authority: "refuse",
  },
];

function section(source: string, heading: string): string {
  const lines = source.split("\n");
  const start = lines.findIndex((line) => {
    const trimmed = line.trim();
    return trimmed === `## ${heading}` || trimmed === `### ${heading}`;
  });
  expect(start, `missing section ${heading}`).toBeGreaterThanOrEqual(0);
  const currentDepth = lines[start]!.trim().startsWith("### ") ? 3 : 2;
  const end = lines.findIndex((line, index) => {
    if (index <= start) return false;
    const trimmed = line.trim();
    return (currentDepth === 2 && trimmed.startsWith("## "))
      || (currentDepth === 3 && (trimmed.startsWith("## ") || trimmed.startsWith("### ")));
  });
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n");
}

function parseTable<T extends Record<string, string>>(
  source: string,
  expectedColumns: readonly string[],
): T[] {
  const rows = source
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith("|") && line.endsWith("|"))
    .map((line) => line.slice(1, -1).split("|").map((cell) => cell.trim()));
  expect(rows.length).toBeGreaterThanOrEqual(3);
  expect(rows[0]).toEqual(expectedColumns);
  expect(rows[1]!.every((cell) => /^-+$/u.test(cell))).toBe(true);
  return rows.slice(2).map((cells) => {
    expect(cells).toHaveLength(expectedColumns.length);
    return Object.fromEntries(expectedColumns.map((column, index) => [column, cells[index]!])) as T;
  });
}

describe("hosted scan security review", () => {
  const plan = readFileSync(
    new URL("../../../docs/plans/hosted-scan-registration.md", import.meta.url),
    "utf8",
  );
  const rows = parseTable<HostedScanReviewRow>(
    section(plan, "Security-Review Checklist for Isolation Boundary"),
    [
      "check_id",
      "boundary",
      "authority_source",
      "renderer_supplied",
      "deletion_authority",
      "refusal_without_authority",
    ],
  );

  it("hosted_scan_isolation_boundary_security_checklist", () => {
    const byId = new Map(rows.map((row) => [row.check_id, row]));
    expect(rows).toEqual(expectedRows);
    for (const expected of expectedRows) {
      expect(byId.get(expected.check_id), expected.check_id).toEqual(expected);
    }
    expect(rows.every((row) => row.renderer_supplied === "no")).toBe(true);
    expect(rows.every((row) => row.deletion_authority === "no")).toBe(true);
  });

  it("src/hosted-scan-security-review.test.ts", () => {
    const trustedNativeBindings = rows.filter((row) => row.authority_source === "native_state");
    expect(trustedNativeBindings.map((row) => row.check_id).sort()).toEqual([
      "hosted_active_context",
      "hosted_identity_unlock",
      "hosted_operator_binding",
      "hosted_scope_binding",
    ]);
    expect(
      rows.find((row) => row.check_id === "hosted_credential_handling"),
    ).toMatchObject({
      authority_source: "unavailable_to_command",
      refusal_without_authority: "refuse",
    });
    expect(rows.find((row) => row.check_id === "hosted_delete_boundary")).toMatchObject({
      authority_source: "not_present_in_scan_port",
      deletion_authority: "no",
      refusal_without_authority: "refuse",
    });
  });

  it("records the f78 independent review as fail-closed and scan-only", () => {
    const review = section(plan, "Independent Review Pass f78");
    const compact = review.replace(/\s+/gu, " ");
    expect(compact).toContain("Verdict: pass");
    expect(compact).toContain("registration stays limited to");
    expect(compact).toContain("renderer can request reachability only");
    expect(compact).toContain("remains a refusal");
    expect(compact).toContain("must not be mapped to an empty scan");
    expect(compact).toContain("content-free shape metadata");
    expect(compact).toContain("not row locators");
    expect(review).not.toMatch(/\bpreview_discord_guided_deletion\b|\bexecute_discord_guided_deletion\b|\bexecute_mass_cleanup_batch\b/u);
  });
});
