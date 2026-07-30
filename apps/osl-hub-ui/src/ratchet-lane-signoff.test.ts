import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type RatchetSignoffRow = {
  remediation_id: string;
  remediation: string;
  closed: "yes" | "no";
  reviewer_basis: string;
  enables_rn: "yes" | "no";
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

describe("ratchet lane reviewer signoff", () => {
  it("reviewer_signoff_confirms_ratchet_remediations_closed", () => {
    const report = readFileSync(
      new URL("../../../docs/reports/ratchet-lane-2026-07-26.md", import.meta.url),
      "utf8",
    );
    const rows = parseTable<RatchetSignoffRow>(
      section(report, "Reviewer Sign-Off: Ratchet Remediations Closed"),
      [
        "remediation_id",
        "remediation",
        "closed",
        "reviewer_basis",
        "enables_rn",
      ],
    );
    expect(rows).toHaveLength(4);
    expect(rows.every((row) => row.closed === "yes")).toBe(true);
    expect(rows.every((row) => row.enables_rn === "no")).toBe(true);
    expect(new Set(rows.map((row) => row.remediation_id))).toEqual(new Set([
      "b2_wire_gate",
      "b3_replay_recovery",
      "b4_bool_seam",
      "b4_monotone_pin",
    ]));
    expect(rows.map((row) => row.reviewer_basis).sort()).toEqual([
      "disabled_gate_review",
      "exhaustive_pin_policy_matrix",
      "mutation_proven_negative_controls",
      "replay_recovery_negative_tests",
    ]);
  });
});
