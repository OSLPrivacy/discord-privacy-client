import {
  execFileSync,
} from "node:child_process";
import {
  generateKeyPairSync,
} from "node:crypto";
import {
  symlink,
  writeFile,
  mkdtemp,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  TRUSTED_DEPLOYMENT_EVIDENCE_PRODUCERS,
  verifyDeploymentEvidenceReceipt,
} from "./deployment-evidence-receipt-contract.mjs";
import {
  consumeDeploymentEvidenceOnce,
  loadCommittedMigrationClosure,
  readDeploymentEvidenceReceipt,
} from "./deployment-evidence-receipt-io.mjs";
import {
  DEPLOYMENT_FIXTURE_ARCHIVE,
  DEPLOYMENT_FIXTURE_COMMIT,
  DEPLOYMENT_FIXTURE_MIGRATIONS,
  DEPLOYMENT_FIXTURE_NOW,
  TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
  deploymentEvidenceEnvelope,
  deploymentEvidenceExpectation,
  deploymentEvidencePayload,
  signDeploymentEvidencePayload,
} from "./deployment-evidence-test-fixture.js";

const REPO_ROOT = path.resolve(
  fileURLToPath(new URL("../..", import.meta.url)),
);

function resign(payload: Record<string, any>) {
  return signDeploymentEvidencePayload(payload);
}

describe("producer-owned deployment evidence receipt", () => {
  it("admits nonempty exact Artifact B and Artifact A producer fixtures", () => {
    const finalReceipt = verifyDeploymentEvidenceReceipt(
      deploymentEvidenceEnvelope("B"),
      deploymentEvidenceExpectation("B"),
      {
        trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      },
    );
    expect(finalReceipt).toMatchObject({
      producer_identity: "test://independent-release-producer",
      payload: {
        expected_commit: DEPLOYMENT_FIXTURE_COMMIT,
        archive_id: DEPLOYMENT_FIXTURE_ARCHIVE,
        artifact: "B",
        migrations: [
          { applied_order: 1 },
          { applied_order: 2 },
        ],
        database: {
          environment: "production",
          schema_object_count: 17,
        },
        worker: {
          sender_filter_route: {
            status: 200,
            item_count: 1,
          },
        },
        sender_filter: {
          advertised: true,
          version: 1,
        },
      },
    });
    expect(finalReceipt.receipt_sha256).toMatch(/^[0-9a-f]{64}$/);

    const bridgeReceipt = verifyDeploymentEvidenceReceipt(
      deploymentEvidenceEnvelope("A"),
      deploymentEvidenceExpectation("A"),
      {
        trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      },
    );
    expect(bridgeReceipt.payload).toMatchObject({
      artifact: "A",
      migrations: [{ applied_order: 1 }],
      worker: {
        sender_filter_route: {
          status: 503,
          item_count: 0,
          filtered_sender_id: null,
        },
      },
      sender_filter: {
        advertised: false,
        version: null,
      },
    });
  });

  it("refuses caller-written and internally self-consistent forged envelopes", () => {
    expect(Object.keys(TRUSTED_DEPLOYMENT_EVIDENCE_PRODUCERS)).toHaveLength(0);
    expect(() =>
      verifyDeploymentEvidenceReceipt(
        deploymentEvidenceEnvelope(),
        deploymentEvidenceExpectation(),
        { nowMs: DEPLOYMENT_FIXTURE_NOW },
      ),
    ).toThrow(/producer is not independently trusted/);

    const attacker = generateKeyPairSync("ed25519");
    const forged = signDeploymentEvidencePayload(
      deploymentEvidencePayload(),
      {
        keyId: "test-independent-release-producer",
        privateKey: attacker.privateKey,
      },
    );
    expect(() =>
      verifyDeploymentEvidenceReceipt(
        forged,
        deploymentEvidenceExpectation(),
        {
          trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
          nowMs: DEPLOYMENT_FIXTURE_NOW,
        },
      ),
    ).toThrow(/producer signature is invalid/);
  });

  it("refuses missing, empty, commit, archive, and Artifact B migration mutations", () => {
    const missing = deploymentEvidenceEnvelope();
    delete (missing.payload as Record<string, unknown>).database;
    expect(() =>
      verifyDeploymentEvidenceReceipt(
        missing,
        deploymentEvidenceExpectation(),
        {
          trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
          nowMs: DEPLOYMENT_FIXTURE_NOW,
        },
      ),
    ).toThrow(/payload fields are not exact|payload digest mismatch/);

    const cases: Array<[string, (payload: Record<string, any>) => void, RegExp]> = [
      [
        "empty migrations",
        (payload) => {
          payload.migrations = [];
        },
        /migration evidence is empty/,
      ],
      [
        "wrong commit",
        (payload) => {
          payload.expected_commit = "f".repeat(40);
        },
        /commit, archive, artifact, or bundle mismatch/,
      ],
      [
        "wrong archive",
        (payload) => {
          payload.archive_id = "e".repeat(64);
        },
        /commit, archive, artifact, or bundle mismatch/,
      ],
      [
        "wrong migration order",
        (payload) => {
          payload.migrations.reverse();
        },
        /migration order or digest mismatch/,
      ],
      [
        "missing 0031",
        (payload) => {
          payload.migrations.pop();
        },
        /exact ordered 0030 then 0031 tail/,
      ],
    ];
    for (const [_name, mutate, message] of cases) {
      const payload = deploymentEvidencePayload();
      mutate(payload);
      expect(() =>
        verifyDeploymentEvidenceReceipt(
          resign(payload),
          deploymentEvidenceExpectation(),
          {
            trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
            nowMs: DEPLOYMENT_FIXTURE_NOW,
          },
        ),
      ).toThrow(message);
    }
  });

  it("refuses database, schema, Worker, route, capability, and A/B isolation mismatches", () => {
    const cases: Array<[string, (payload: Record<string, any>) => void, RegExp]> = [
      [
        "wrong environment",
        (payload) => {
          payload.database.environment = "preview";
        },
        /database identity mismatch/,
      ],
      [
        "empty schema",
        (payload) => {
          payload.database.schema_object_count = 0;
        },
        /schema evidence is empty/,
      ],
      [
        "wrong schema query",
        (payload) => {
          payload.database.schema_query_sha256 = "0".repeat(64);
        },
        /database query contract mismatch/,
      ],
      [
        "schema fingerprint mismatch",
        (payload) => {
          payload.database.schema_fingerprint_sha256 = "e".repeat(64);
        },
        /output or schema fingerprint mismatch/,
      ],
      [
        "wrong Worker version",
        (payload) => {
          payload.worker.version_id =
            "99999999-8888-4777-8666-555555555555";
        },
        /Worker identity or bundle mismatch/,
      ],
      [
        "route drift",
        (payload) => {
          payload.worker.sender_filter_route.path = "/v1/control-inbox";
        },
        /sender-filter route contract mismatch/,
      ],
      [
        "positive starvation",
        (payload) => {
          payload.worker.sender_filter_route.item_count = 0;
        },
        /route proof is empty or mismatched/,
      ],
      [
        "capability mismatch",
        (payload) => {
          payload.sender_filter.version = 2;
        },
        /capability advertisement mismatch/,
      ],
      [
        "cross-contaminated artifacts",
        (payload) => {
          payload.artifact_isolation.artifact_a.active = true;
          payload.artifact_isolation.artifact_a.observed_version_id =
            payload.worker.version_id;
        },
        /Artifact A\/B isolation mismatch/,
      ],
    ];
    for (const [_name, mutate, message] of cases) {
      const payload = deploymentEvidencePayload();
      mutate(payload);
      expect(() =>
        verifyDeploymentEvidenceReceipt(
          resign(payload),
          deploymentEvidenceExpectation(),
          {
            trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
            nowMs: DEPLOYMENT_FIXTURE_NOW,
          },
        ),
      ).toThrow(message);
    }
  });

  it("refuses stale, future, reversed, and overlong producer timestamps", () => {
    const cases: Array<[(payload: Record<string, any>) => void, RegExp]> = [
      [
        (payload) => {
          payload.timestamps.issued_at = "2026-07-27T11:55:00.000Z";
        },
        /timestamps are out of order|stale or future-dated/,
      ],
      [
        (payload) => {
          payload.timestamps.issued_at = "2026-07-27T12:01:00.000Z";
        },
        /future-dated/,
      ],
      [
        (payload) => {
          payload.timestamps.worker_deployed_at =
            "2026-07-27T11:59:51.000Z";
        },
        /timestamps are out of order/,
      ],
      [
        (payload) => {
          payload.timestamps.action_started_at =
            "2026-07-27T11:40:00.000Z";
        },
        /action interval is too long/,
      ],
    ];
    for (const [mutate, message] of cases) {
      const payload = deploymentEvidencePayload();
      mutate(payload);
      expect(() =>
        verifyDeploymentEvidenceReceipt(
          resign(payload),
          deploymentEvidenceExpectation(),
          {
            trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
            nowMs: DEPLOYMENT_FIXTURE_NOW,
          },
        ),
      ).toThrow(message);
    }
  });

  it("atomically consumes the signed producer chain and refuses replay", async () => {
    const ledgerDirectory = await mkdtemp(
      path.join(tmpdir(), "deployment-evidence-ledger-"),
    );
    const first = verifyDeploymentEvidenceReceipt(
      deploymentEvidenceEnvelope(),
      deploymentEvidenceExpectation(),
      {
        trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      },
    );
    await expect(
      consumeDeploymentEvidenceOnce(first, { ledgerDirectory }),
    ).resolves.toMatchObject({ sequence: 1 });
    await expect(
      consumeDeploymentEvidenceOnce(first, { ledgerDirectory }),
    ).rejects.toThrow(/replayed or out of chain/);

    const secondPayload = deploymentEvidencePayload();
    secondPayload.producer_sequence = 2;
    secondPayload.previous_receipt_sha256 = first.receipt_sha256;
    secondPayload.producer_run_id =
      "cccccccc-dddd-4eee-8fff-000000000000";
    const second = verifyDeploymentEvidenceReceipt(
      resign(secondPayload),
      deploymentEvidenceExpectation(),
      {
        trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      },
    );
    await expect(
      consumeDeploymentEvidenceOnce(second, { ledgerDirectory }),
    ).resolves.toMatchObject({
      sequence: 2,
      receipt_sha256: second.receipt_sha256,
    });
  });

  it("reads only a nonempty absolute regular receipt file", async () => {
    const directory = await mkdtemp(
      path.join(tmpdir(), "deployment-evidence-file-"),
    );
    const receiptPath = path.join(directory, "receipt.json");
    await writeFile(
      receiptPath,
      JSON.stringify(deploymentEvidenceEnvelope()),
    );
    await expect(readDeploymentEvidenceReceipt(receiptPath)).resolves.toMatchObject({
      payload: { expected_commit: DEPLOYMENT_FIXTURE_COMMIT },
    });
    await expect(
      readDeploymentEvidenceReceipt("relative-receipt.json"),
    ).rejects.toThrow(/must be absolute/);

    const emptyPath = path.join(directory, "empty.json");
    await writeFile(emptyPath, "");
    await expect(readDeploymentEvidenceReceipt(emptyPath)).rejects.toThrow(
      /empty or oversized/,
    );
    const linkPath = path.join(directory, "receipt-link.json");
    await symlink(receiptPath, linkPath);
    await expect(readDeploymentEvidenceReceipt(linkPath)).rejects.toThrow(
      /not a regular file/,
    );
  });

  it("requires a nonempty exact committed migration expectation", () => {
    const empty = deploymentEvidenceExpectation();
    empty.expectedMigrations = [];
    expect(() =>
      verifyDeploymentEvidenceReceipt(
        deploymentEvidenceEnvelope(),
        empty,
        {
          trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
          nowMs: DEPLOYMENT_FIXTURE_NOW,
        },
      ),
    ).toThrow(/committed migration closure is empty/);

    const mismatch = deploymentEvidenceExpectation();
    mismatch.expectedMigrations = structuredClone(
      DEPLOYMENT_FIXTURE_MIGRATIONS,
    );
    mismatch.expectedMigrations[1].sha256 = "0".repeat(64);
    expect(() =>
      verifyDeploymentEvidenceReceipt(
        deploymentEvidenceEnvelope(),
        mismatch,
        {
          trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
          nowMs: DEPLOYMENT_FIXTURE_NOW,
        },
      ),
    ).toThrow(/migration order or digest mismatch/);
  });

  it("loads the complete nonempty ordered migration closure from the exact commit", () => {
    const head = execFileSync(
      "git",
      ["-C", REPO_ROOT, "rev-parse", "HEAD"],
      { encoding: "utf8" },
    ).trim();
    const migrations = loadCommittedMigrationClosure(REPO_ROOT, head);
    expect(migrations).toHaveLength(31);
    expect(migrations[0].name).toMatch(/^0001_/);
    expect(migrations.at(-1)?.name).toBe(
      "0031_control_inbox_sender_retention.sql",
    );
    expect(
      migrations.every((entry) => /^[0-9a-f]{64}$/.test(entry.sha256)),
    ).toBe(true);
    expect(new Set(migrations.map((entry) => entry.sha256)).size).toBe(
      migrations.length,
    );
  });
});
