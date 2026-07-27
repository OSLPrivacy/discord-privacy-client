import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  createSenderFilterDeploymentReceipt,
  evaluateSenderFilterLiveEvidence,
  SENDER_FILTER_EVIDENCE_FORMAT,
  SENDER_FILTER_LIVE_NULL_FIXTURE,
  SENDER_FILTER_SOURCE_FILES,
  validateSenderFilterSourceClosure,
} from "./sender-filter-deployment-contract.mjs";
import {
  loadSenderFilterReceiptInputs,
  parseSenderFilterReceiptArgs,
  runSenderFilterReceiptCli,
} from "./create-sender-filter-deployment-receipt.mjs";

const COMMIT = "a".repeat(40);
const NOW = Date.parse("2026-07-27T12:00:00.000Z");
const REPO_ROOT = path.resolve(
  fileURLToPath(new URL("../..", import.meta.url)),
);

async function committedSourceValues() {
  return Object.fromEntries(
    await Promise.all(
      Object.keys(SENDER_FILTER_SOURCE_FILES).map(async (sourcePath) => [
        sourcePath,
        await readFile(path.join(REPO_ROOT, sourcePath)),
      ]),
    ),
  );
}

function positiveEvidence(overrides: Record<string, unknown> = {}) {
  return {
    format: SENDER_FILTER_EVIDENCE_FORMAT,
    evidence_tier: "verified-live-read-only",
    database: "osl-keyserver-prod",
    environment: "production",
    captured_started_at: "2026-07-27T11:59:55.000Z",
    captured_finished_at: "2026-07-27T11:59:58.000Z",
    active_worker_version: "11111111-2222-4333-8444-555555555555",
    worker_commit: COMMIT,
    migration_0031_present: true,
    capability_table_exists: 1,
    control_inbox_sender_disposition: 1,
    health_probe: {
      status: 200,
      ok: true,
      capability: 1,
    },
    route_probe: {
      status: 200,
      requested_sender_id: "sender-fixture",
      signed_sender_id: "sender-fixture",
      filtered_sender_id: "sender-fixture",
      items: [
        {
          sender_id: "sender-fixture",
          bundle_b64: "AQ==",
        },
      ],
      filtered_sender_delivery: {
        live: 1,
        retryable: 0,
        quarantined: 0,
        retired: 0,
      },
    },
    ...overrides,
  };
}

describe("source-only signed sender-filter deployment admission", () => {
  it("binds a nonempty exact migration, route, capability, and behavior closure", async () => {
    const files = validateSenderFilterSourceClosure(
      await committedSourceValues(),
    );
    expect(files).toHaveLength(7);
    expect(files.every((file) => file.bytes > 0)).toBe(true);
    expect(files.map((file) => file.role)).toEqual(
      expect.arrayContaining([
        "migration-0031",
        "signed-sender-filter-route",
        "capability-route",
        "signed-filter-canonical-bytes",
        "nonempty-route-behavior-fixture",
      ]),
    );
  });

  it("has a nonempty positive live fixture without authorizing source-only output", async () => {
    expect(
      evaluateSenderFilterLiveEvidence(positiveEvidence(), COMMIT, NOW),
    ).toEqual({
      would_admit_with_trusted_live_evidence: true,
      refusal_reasons: [],
    });
    const receipt = createSenderFilterDeploymentReceipt({
      anchor: {
        commit: COMMIT,
        repository_tree: "b".repeat(40),
        keyserver_tree: "c".repeat(40),
      },
      fileValues: await committedSourceValues(),
      liveEvidence: positiveEvidence(),
      nowMs: NOW,
    });
    expect(receipt.source_contract_admitted).toBe(true);
    expect(receipt.would_admit_with_trusted_live_evidence).toBe(true);
    expect(receipt.deployment_admitted).toBe(false);
    expect(receipt.source.files.every((file) => file.bytes > 0)).toBe(true);
    expect(receipt.payload_sha256).toMatch(/^[0-9a-f]{64}$/);
  });

  it("refuses missing source and exact-byte mismatches", async () => {
    const missing = await committedSourceValues();
    delete missing[
      "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql"
    ];
    expect(() => validateSenderFilterSourceClosure(missing)).toThrow(
      /fields are not exact/,
    );

    const mismatched = await committedSourceValues();
    mismatched["keyserver-cf/src/endpoints/control-inbox.ts"] = Buffer.from(
      `${mismatched["keyserver-cf/src/endpoints/control-inbox.ts"].toString()}\n// ignored sender filter\n`,
    );
    expect(() => validateSenderFilterSourceClosure(mismatched)).toThrow(
      /source hash mismatch.*control-inbox\.ts/,
    );

    const missingEvidence = positiveEvidence();
    delete missingEvidence.health_probe;
    expect(() =>
      evaluateSenderFilterLiveEvidence(missingEvidence, COMMIT, NOW),
    ).toThrow(/live evidence fields are not exact/);
  });

  it("refuses commit, route echo, and positive-starvation mismatches", () => {
    expect(
      evaluateSenderFilterLiveEvidence(
        positiveEvidence({ worker_commit: "b".repeat(40) }),
        COMMIT,
        NOW,
      ).refusal_reasons,
    ).toContain("live-worker-commit-mismatch");

    const routeMismatch = positiveEvidence();
    (routeMismatch.route_probe as Record<string, unknown>).filtered_sender_id =
      "different-sender";
    expect(
      evaluateSenderFilterLiveEvidence(routeMismatch, COMMIT, NOW)
        .refusal_reasons,
    ).toContain("sender-filter-route-echo-mismatch");

    const starved = positiveEvidence();
    (starved.route_probe as Record<string, unknown>).items = [];
    expect(
      evaluateSenderFilterLiveEvidence(starved, COMMIT, NOW).refusal_reasons,
    ).toContain("sender-filter-route-positive-starved");
  });

  it("refuses stale evidence and the exact historical live-null disposition", async () => {
    expect(
      evaluateSenderFilterLiveEvidence(
        positiveEvidence({
          captured_started_at: "2026-07-27T11:55:00.000Z",
          captured_finished_at: "2026-07-27T11:55:01.000Z",
        }),
        COMMIT,
        NOW,
      ).refusal_reasons,
    ).toContain("live-evidence-stale");

    const liveNull = JSON.parse(
      await readFile(
        path.join(REPO_ROOT, SENDER_FILTER_LIVE_NULL_FIXTURE),
        "utf8",
      ),
    );
    const refusal = evaluateSenderFilterLiveEvidence(
      liveNull,
      COMMIT,
      NOW + 5 * 60_000,
    );
    expect(refusal.would_admit_with_trusted_live_evidence).toBe(false);
    expect(refusal.refusal_reasons).toEqual(
      expect.arrayContaining([
        "live-worker-commit-unmapped",
        "live-evidence-stale",
        "migration-0031-absent",
        "capability-table-absent",
        "live-disposition-null",
        "health-capability-missing",
        "signed-sender-filter-route-missing",
      ]),
    );
  });

  it("requires one exact current commit and emits only a non-authorizing receipt", async () => {
    expect(
      parseSenderFilterReceiptArgs(["--expected-commit", COMMIT]),
    ).toEqual({ expectedCommit: COMMIT });
    expect(() =>
      parseSenderFilterReceiptArgs(["--expected-commit", "HEAD"]),
    ).toThrow(/usage/);

    const gitRun = (
      _repoRoot: string,
      args: string[],
      encoding: BufferEncoding | null = "utf8",
    ): string | Buffer => {
      const command = args.join(" ");
      if (command === `rev-parse --verify ${COMMIT}^{commit}`) return `${COMMIT}\n`;
      if (command === "rev-parse HEAD") return `${"b".repeat(40)}\n`;
      throw new Error(`unexpected git command: ${command} (${encoding})`);
    };
    expect(() =>
      loadSenderFilterReceiptInputs("/repo", COMMIT, gitRun),
    ).toThrow(/stale relative to HEAD/);

    const fileValues = await committedSourceValues();
    let output = "";
    const receipt = runSenderFilterReceiptCli(
      ["--expected-commit", COMMIT],
      {
        loadInputs: () => ({
          anchor: {
            commit: COMMIT,
            repository_tree: "b".repeat(40),
            keyserver_tree: "c".repeat(40),
          },
          fileValues,
          liveEvidence: positiveEvidence(),
        }),
        write: (text: string) => {
          output += text;
        },
        now: () => NOW,
      },
    );
    expect(receipt.deployment_admitted).toBe(false);
    expect(output).toContain('"admission_scope": "source-only-non-authorizing"');
  });
});
