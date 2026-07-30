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

function section(source: string, heading: string): string {
  const marker = `## ${heading}`;
  const start = source.split("\n").findIndex((line) => line.trim() === marker);
  expect(start, `missing section ${heading}`).toBeGreaterThanOrEqual(0);
  const lines = source.split("\n");
  const end = lines.findIndex((line, index) => index > start && line.startsWith("## "));
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
    for (const required of [
      "hosted_identity_unlock",
      "hosted_active_context",
      "hosted_scope_binding",
      "hosted_operator_binding",
      "hosted_credential_handling",
      "hosted_delete_boundary",
    ]) {
      expect(byId.get(required)?.refusal_without_authority, required).toBe("refuse");
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
});
