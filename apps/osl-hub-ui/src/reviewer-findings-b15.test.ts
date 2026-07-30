import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type FindingRow = {
  finding_id: string;
  severity: string;
  status: string;
  owner_unit: string;
  source: string;
  required_remediation: string;
  rn_enabled: "yes" | "no";
};

type GuardrailRow = {
  guardrail_id: string;
  absent_or_invalid_condition: string;
  required_outcome: string;
  rn_enabled: "yes" | "no";
};

function reportSection(report: string, heading: string): string {
  const lines = report.split("\n");
  const start = lines.findIndex((line) => line.trim() === `## ${heading}`);
  expect(start, `missing section ${heading}`).toBeGreaterThanOrEqual(0);
  const end = lines.findIndex((line, index) => index > start && line.startsWith("## "));
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n");
}

function tableRows<T extends Record<string, string>>(
  markdown: string,
  expectedColumns: readonly string[],
): T[] {
  const rows = markdown
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith("|") && line.endsWith("|"))
    .map((line) => line.slice(1, -1).split("|").map((cell) => cell.trim()));
  expect(rows.length).toBeGreaterThanOrEqual(3);
  expect(rows[0]).toEqual(expectedColumns);
  expect(rows[1]?.every((cell) => /^-+$/u.test(cell))).toBe(true);
  return rows.slice(2).map((cells) => {
    expect(cells).toHaveLength(expectedColumns.length);
    return Object.fromEntries(expectedColumns.map((column, index) => [column, cells[index]!])) as T;
  });
}

describe("reviewer findings b15", () => {
  it("reviewer_produces_findings_report", () => {
    const report = readFileSync(
      new URL("../../../docs/reports/reviewer-findings-b15.md", import.meta.url),
      "utf8",
    );
    const findings = tableRows<FindingRow>(reportSection(report, "Findings"), [
      "finding_id",
      "severity",
      "status",
      "owner_unit",
      "source",
      "required_remediation",
      "rn_enabled",
    ]);
    const guardrails = tableRows<GuardrailRow>(reportSection(report, "Authority guardrails"), [
      "guardrail_id",
      "absent_or_invalid_condition",
      "required_outcome",
      "rn_enabled",
    ]);

    expect(findings).toEqual([
      {
        finding_id: "b20_session_reset_symptom_deadlock",
        severity: "high",
        status: "open",
        owner_unit: "b20",
        source: "ratchet-lane-review",
        required_remediation: "Honor authenticated fresh SESSION_RESET control messages without requiring a same-side local v4 decrypt failure, while keeping replay, staleness and honor-throttle refusal intact.",
        rn_enabled: "no",
      },
      {
        finding_id: "b20_recovery_result_observability",
        severity: "medium",
        status: "open",
        owner_unit: "b20",
        source: "ratchet-lane-review",
        required_remediation: "Return an explicit applied-versus-ignored sentinel from SESSION_RESET handling so callers cannot mistake a refused recovery control message for user plaintext.",
        rn_enabled: "no",
      },
      {
        finding_id: "b36_requires_re_review_signoff",
        severity: "medium",
        status: "pending_re_review",
        owner_unit: "b36",
        source: "dependency-chain",
        required_remediation: "Re-review the b20 command-surface remediation and record sign-off only after the named IPC acceptance test exists and exercises the closed behavior.",
        rn_enabled: "no",
      },
    ]);
    expect(guardrails).toEqual([
      {
        guardrail_id: "authenticated_control_missing",
        absent_or_invalid_condition: "SESSION_RESET authentication",
        required_outcome: "ignore_control",
        rn_enabled: "no",
      },
      {
        guardrail_id: "freshness_or_replay_invalid",
        absent_or_invalid_condition: "fresh timestamp or unused nonce",
        required_outcome: "ignore_control",
        rn_enabled: "no",
      },
      {
        guardrail_id: "binding_absent",
        absent_or_invalid_condition: "valid local peer binding",
        required_outcome: "refuse_recovery",
        rn_enabled: "no",
      },
      {
        guardrail_id: "honor_throttle_exceeded",
        absent_or_invalid_condition: "recovery honor budget",
        required_outcome: "ignore_control",
        rn_enabled: "no",
      },
    ]);
    expect([...findings, ...guardrails].every((row) => row.rn_enabled === "no")).toBe(true);
  });
});
