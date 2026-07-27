import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  createSenderFilterDeploymentReceipt,
  SENDER_FILTER_EVIDENCE_FORMAT,
  SENDER_FILTER_SOURCE_FILES,
  sha256,
} from "./sender-filter-deployment-contract.mjs";
import {
  parseTrustedReleaseArgs,
  runTrustedReleaseSelectionCli,
} from "./select-trusted-release.mjs";
import {
  productionActionRefusal,
  runProductionActionRefusalCli,
} from "./refuse-unadmitted-production-action.mjs";
import {
  selectTrustedRelease,
  TRUSTED_RELEASE_SELECTION_FORMAT,
} from "./trusted-release-selection-contract.mjs";

const COMMIT = "a".repeat(40);
const NOW = Date.parse("2026-07-27T12:00:00.000Z");
const ACTIVE_VERSION = "11111111-2222-4333-8444-555555555555";
const DEPLOYMENT_ID = "66666666-7777-4888-9999-aaaaaaaaaaaa";
const REPO_ROOT = path.resolve(
  fileURLToPath(new URL("../..", import.meta.url)),
);

async function sourceInputs() {
  return {
    anchor: {
      commit: COMMIT,
      repository_tree: "b".repeat(40),
      keyserver_tree: "c".repeat(40),
    },
    fileValues: Object.fromEntries(
      await Promise.all(
        Object.keys(SENDER_FILTER_SOURCE_FILES).map(async (sourcePath) => [
          sourcePath,
          await readFile(path.join(REPO_ROOT, sourcePath)),
        ]),
      ),
    ),
    liveEvidence: {
      format: SENDER_FILTER_EVIDENCE_FORMAT,
      evidence_tier: "verified-live-read-only",
      database: "osl-keyserver-prod",
      environment: "production",
      captured_started_at: "2026-07-27T11:59:55.000Z",
      captured_finished_at: "2026-07-27T11:59:58.000Z",
      active_worker_version: ACTIVE_VERSION,
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
        requested_sender_id: "sender-positive",
        signed_sender_id: "sender-positive",
        filtered_sender_id: "sender-positive",
        items: [{ sender_id: "sender-positive", bundle_b64: "AQ==" }],
        filtered_sender_delivery: {
          live: 1,
          retryable: 0,
          quarantined: 0,
          retired: 0,
        },
      },
    },
  };
}

function readinessReceipt(overrides: Record<string, unknown> = {}) {
  return {
    format: "osl.keyserver.readiness-admission.v2",
    admitted: true,
    expected_commit: COMMIT,
    archive_id: "d".repeat(64),
    artifact: "B",
    database: "osl-keyserver-prod",
    database_id: "1de837cd-3bf6-4d33-be82-12d358523600",
    environment: "production",
    active_worker_version: ACTIVE_VERSION,
    deployment_id: DEPLOYMENT_ID,
    captured_started_at: "2026-07-27T11:59:55.000Z",
    captured_finished_at: "2026-07-27T11:59:58.000Z",
    database_unix_time: Math.floor(NOW / 1000),
    schema_query_sha256:
      "a9724ffa17bf964260152ef2c7e79b63250cdc3eaa23426580794900c33cd2a9",
    markers_query_sha256:
      "4ef763198a5ca70a3446f4df772629f4b9f31c36db9ff55087c5024db2b14bfa",
    deployment_status_before_sha256: "1".repeat(64),
    deployment_status_after_sha256: "2".repeat(64),
    capability_table_exists: 1,
    markers: {
      control_inbox_sender_disposition: 1,
      control_inbox_sender_reconciliation_started: null,
    },
    ...overrides,
  };
}

async function selectionInputs() {
  return {
    expectedCommit: COMMIT,
    artifact: "B",
    expectedActiveVersion: ACTIVE_VERSION,
    sourceReceipt: createSenderFilterDeploymentReceipt({
      ...(await sourceInputs()),
      nowMs: NOW,
    }),
    readinessReceipt: readinessReceipt(),
    nowMs: NOW,
  };
}

describe("trusted keyserver release selection", () => {
  it("admits a nonempty exact commit/schema/route fixture but authorizes no action", async () => {
    const selected = selectTrustedRelease(await selectionInputs());
    expect(selected).toMatchObject({
      format: TRUSTED_RELEASE_SELECTION_FORMAT,
      selection_admitted: true,
      execution_authorized: false,
      execution_performed: false,
      expected_commit: COMMIT,
      artifact: "B",
      capability_table_exists: 1,
    });
    expect(selected.route_contract).toMatchObject({
      method: "GET",
      path: "/v1/control-inbox/:user_id",
      signed_query_component: "sender",
      health_capability: "control_inbox_sender_disposition",
      health_capability_version: 1,
    });
    expect(selected.source_payload_sha256).toMatch(/^[0-9a-f]{64}$/);
  });

  it("refuses missing fields and commit or artifact mismatches", async () => {
    const missing = await selectionInputs();
    delete (missing.readinessReceipt as Record<string, unknown>).archive_id;
    expect(() => selectTrustedRelease(missing)).toThrow(
      /readiness receipt fields are not exact/,
    );

    const wrongCommit = await selectionInputs();
    wrongCommit.readinessReceipt = readinessReceipt({
      expected_commit: "9".repeat(40),
    });
    expect(() => selectTrustedRelease(wrongCommit)).toThrow(
      /commit or artifact mismatch/,
    );

    const wrongArtifact = await selectionInputs();
    wrongArtifact.readinessReceipt = readinessReceipt({ artifact: "A" });
    expect(() => selectTrustedRelease(wrongArtifact)).toThrow(
      /commit or artifact mismatch/,
    );

    const unknownArtifact = await selectionInputs();
    unknownArtifact.artifact = "C";
    unknownArtifact.readinessReceipt = readinessReceipt({ artifact: "C" });
    expect(() => selectTrustedRelease(unknownArtifact)).toThrow(
      /artifact must be exactly A or B/,
    );
  });

  it("refuses stale evidence and absent or mismatched Artifact B schema", async () => {
    const stale = await selectionInputs();
    stale.readinessReceipt = readinessReceipt({
      captured_started_at: "2026-07-27T11:55:00.000Z",
      captured_finished_at: "2026-07-27T11:55:01.000Z",
    });
    expect(() => selectTrustedRelease(stale)).toThrow(/stale or future-dated/);

    const missingTable = await selectionInputs();
    missingTable.readinessReceipt = readinessReceipt({
      capability_table_exists: 0,
      markers: {
        control_inbox_sender_disposition: null,
        control_inbox_sender_reconciliation_started: null,
      },
    });
    expect(() => selectTrustedRelease(missingTable)).toThrow(
      /requires the migration 0031 capability table/,
    );

    const wrongMarker = await selectionInputs();
    wrongMarker.readinessReceipt = readinessReceipt({
      markers: {
        control_inbox_sender_disposition: null,
        control_inbox_sender_reconciliation_started: null,
      },
    });
    expect(() => selectTrustedRelease(wrongMarker)).toThrow(
      /requires the migration 0031 capability table/,
    );

    const wrongQuery = await selectionInputs();
    wrongQuery.readinessReceipt = readinessReceipt({
      schema_query_sha256: "0".repeat(64),
    });
    expect(() => selectTrustedRelease(wrongQuery)).toThrow(
      /query contract mismatch/,
    );
  });

  it("refuses route or source payload mutation", async () => {
    const wrongRoute = await selectionInputs();
    wrongRoute.sourceReceipt.route_contract = {
      ...wrongRoute.sourceReceipt.route_contract,
      signed_query_component: "ignored",
    };
    const wrongRoutePayload = { ...wrongRoute.sourceReceipt };
    delete (wrongRoutePayload as { payload_sha256?: string }).payload_sha256;
    wrongRoute.sourceReceipt.payload_sha256 = sha256(
      Buffer.from(JSON.stringify(wrongRoutePayload)),
    );
    expect(() => selectTrustedRelease(wrongRoute)).toThrow(
      /route contract mismatch/,
    );

    const wrongDigest = await selectionInputs();
    wrongDigest.sourceReceipt.payload_sha256 = "0".repeat(64);
    expect(() => selectTrustedRelease(wrongDigest)).toThrow(
      /payload digest mismatch/,
    );
  });

  it("runs source and readiness admission in-process without accepting a receipt file", async () => {
    const args = [
      "--expected-commit",
      COMMIT,
      "--archive-dir",
      "/tmp/nonempty-trusted-archive",
      "--artifact",
      "B",
      "--expected-active-version",
      ACTIVE_VERSION,
    ];
    expect(parseTrustedReleaseArgs(args)).toMatchObject({
      expectedCommit: COMMIT,
      artifact: "B",
      expectedActiveVersion: ACTIVE_VERSION,
    });
    expect(() =>
      parseTrustedReleaseArgs([...args, "--receipt", "/tmp/forged.json"]),
    ).toThrow(/usage/);

    let admittedArgs: string[] = [];
    let output = "";
    const selected = await runTrustedReleaseSelectionCli(args, {
      loadSource: async () => sourceInputs(),
      admit: async (received: string[]) => {
        admittedArgs = received;
        return readinessReceipt();
      },
      now: () => NOW,
      write: (text: string) => {
        output += text;
      },
    });
    expect(admittedArgs).toEqual(args);
    expect(selected.selection_admitted).toBe(true);
    expect(selected.execution_authorized).toBe(false);
    expect(output).toContain('"execution_performed": false');
  });

  it("blocks both package production actions and never exposes an executor", () => {
    expect(productionActionRefusal("deploy")).toMatch(/exact admitted bundle/);
    expect(productionActionRefusal("migrate")).toMatch(
      /active bridge Worker version/,
    );
    let stderr = "";
    expect(
      runProductionActionRefusalCli(["deploy"], {
        writeError: (text: string) => {
          stderr += text;
        },
      }),
    ).toMatchObject({ refused: true, action: "deploy" });
    expect(stderr).toMatch(/production deploy refused/);
    expect(() => productionActionRefusal("other")).toThrow(
      /exactly deploy or migrate/,
    );
  });

  it("keeps package and sender-filter runbook mutation paths fail closed", async () => {
    const packageJson = JSON.parse(
      await readFile(path.join(REPO_ROOT, "keyserver-cf/package.json"), "utf8"),
    );
    expect(packageJson.scripts.deploy).toBe(
      "node scripts/refuse-unadmitted-production-action.mjs deploy",
    );
    expect(packageJson.scripts["db:migrate:prod"]).toBe(
      "node scripts/refuse-unadmitted-production-action.mjs migrate",
    );
    expect(packageJson.scripts["release:select"]).toBe(
      "node scripts/select-trusted-release.mjs",
    );

    const runbook = await readFile(
      path.join(REPO_ROOT, "keyserver-cf/DEPLOY.md"),
      "utf8",
    );
    const senderFilterSection = runbook
      .split("## §11c Migration 0031")[1]
      .split("## §12 Next:")[0];
    expect(senderFilterSection).toContain("execution_authorized=false");
    expect(senderFilterSection).toContain("npm run release:select");
    expect(senderFilterSection).not.toMatch(
      /npx wrangler (?:deploy|d1 migrations apply)/,
    );
  });
});
