import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
// @ts-expect-error - plain-JS operator command, imported for its pure helpers.
import {
  assessRowOwnershipEvidence,
  evidenceKindsUsingForbiddenBasis,
  readRowOwnershipLadder,
} from "./row-ownership-ladder.mjs";

const SCRIPT = join(process.cwd(), "scripts/row-ownership-ladder.mjs");
const LADDER = join(process.cwd(), "ROW-OWNERSHIP-PROOF-LADDER.json");

function runLadder(args: string[]): { status: number; output: string } {
  try {
    return {
      status: 0,
      output: execFileSync("node", [SCRIPT, ...args], {
        encoding: "utf8",
        stdio: "pipe",
      }),
    };
  } catch (error) {
    const e = error as { status?: number; stdout?: string; stderr?: string };
    return {
      status: e.status ?? 1,
      output: `${e.stdout ?? ""}${e.stderr ?? ""}`,
    };
  }
}

describe("row ownership proof ladder", () => {
  it("names ordered evidence kinds, the Discord top proof, and the marking floor", () => {
    const ladder = readRowOwnershipLadder(LADDER);

    expect(ladder.evidence.length).toBeGreaterThanOrEqual(4);
    expect(ladder.evidence.map((kind) => kind.rank)).toEqual([1, 2, 3, 4, 5]);
    expect(ladder.evidence.map((kind) => kind.id)).toEqual([
      "discord_row_account_number_matches_account_panel_number",
      "stable_provider_account_id_cross_check",
      "provider_authenticated_row_owner_claim",
      "signed_osl_challenge_claimant_binding",
      "account_name_or_display_handle",
    ]);

    expect(ladder.lowest_allowed_evidence_kind).toBe(
      "stable_provider_account_id_cross_check",
    );
    expect(evidenceKindsUsingForbiddenBasis(ladder)).toEqual([]);

    const top = ladder.evidence[0];
    expect(top.example_app).toBe("discord");
    expect(top.basis).toEqual([
      "row_account_number",
      "signed_in_account_panel_account_number",
    ]);
    expect(top.why).toContain("numbered account from the row");
    expect(top.why).toContain("signed-in account's own number");

    const nameOnly = ladder.evidence.find(
      (kind) => kind.id === "account_name_or_display_handle",
    );
    expect(nameOnly?.why).toBe(
      "Weakest: a name on its own is weak and may only ever narrow an answer, never make one.",
    );
  });

  it("direct command reads the ladder and refuses evidence below the line by app name", () => {
    const result = runLadder([
      "--ladder",
      LADDER,
      "--app",
      "telegram",
      "--evidence",
      "account_name_or_display_handle",
    ]);

    expect(result.status).toBe(1);
    expect(result.output).toContain("ladder=ROW-OWNERSHIP-PROOF-LADDER.json");
    expect(result.output).toContain(
      "refused app=telegram evidence=account_name_or_display_handle below_minimum=stable_provider_account_id_cross_check",
    );
    expect(result.output).toContain(
      "a name on its own is weak and may only ever narrow an answer, never make one",
    );
  });

  it("direct command reports the measured ladder counts", () => {
    const result = runLadder(["--ladder", LADDER, "--list"]);

    expect(result.status).toBe(0);
    expect(result.output).toContain("evidence_kind_count=5");
    expect(result.output).toContain(
      "top_discord_compares=row_account_number,signed_in_account_panel_account_number",
    );
    expect(result.output).toContain(
      "position_or_bubble_colour_evidence_kinds=0",
    );
  });

  it("allows the lowest evidence kind that clears the marking floor", () => {
    const ladder = readRowOwnershipLadder(LADDER);
    expect(
      assessRowOwnershipEvidence(
        ladder,
        "signal",
        "stable_provider_account_id_cross_check",
      ),
    ).toEqual({
      ok: true,
      line: "allowed app=signal evidence=stable_provider_account_id_cross_check minimum=stable_provider_account_id_cross_check",
    });
  });
});
