import { describe, expect, it } from "vitest";
import {
  assertReadinessRefusal,
  parseRefusalArgs,
  runRefusalCli,
} from "./assert-readiness-refusal.mjs";

const LIVE_ABSENT = Object.freeze({
  capability_table_exists: 0,
  control_inbox_sender_disposition: null,
  control_inbox_sender_reconciliation_started: null,
});

describe("source-only migration 0031 refusal gate", () => {
  it("confirms refusal of Artifact B and generic migration-dependent workers", () => {
    for (const target of [
      "artifact-b-final",
      "migration-dependent-worker",
    ]) {
      expect(assertReadinessRefusal(target, LIVE_ABSENT)).toMatch(
        /capability table and exact disposition marker/,
      );
    }
  });

  it("fails if an unsafe target is no longer refused", () => {
    expect(() =>
      assertReadinessRefusal("artifact-b-final", {
        capability_table_exists: 1,
        control_inbox_sender_disposition: 1,
        control_inbox_sender_reconciliation_started: null,
      })
    ).toThrow(/was not refused/);
  });

  it("accepts only exact source-only evidence flags and no caller file", () => {
    expect(
      parseRefusalArgs([
        "--candidate",
        "artifact-b-final",
        "--capability-table-exists",
        "0",
        "--disposition-marker",
        "null",
        "--reconciliation-marker",
        "null",
      ]),
    ).toEqual({
      candidate: "artifact-b-final",
      evidence: LIVE_ABSENT,
    });
    expect(() =>
      parseRefusalArgs([
        "--candidate",
        "artifact-b-final",
        "--capability-table-exists",
        "0",
        "--disposition-marker",
        "null",
        "--reconciliation-marker",
        "null",
        "--evidence",
        "/tmp/caller.json",
      ])
    ).toThrow(/usage/);
  });

  it("emits a nonempty runbook refusal receipt", () => {
    let output = "";
    expect(
      runRefusalCli(
        [
          "--candidate",
          "migration-dependent-worker",
          "--capability-table-exists",
          "0",
          "--disposition-marker",
          "null",
          "--reconciliation-marker",
          "null",
        ],
        (text) => {
          output += text;
        },
      ),
    ).toBe(true);
    expect(output).toMatch(
      /^refusal confirmed: migration-dependent-worker: .*migration 0031/,
    );
  });
});
