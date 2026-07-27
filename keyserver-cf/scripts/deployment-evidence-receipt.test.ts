import { execFileSync } from "node:child_process";
import { generateKeyPairSync } from "node:crypto";
import {
  mkdtemp,
  rename,
  symlink,
  writeFile,
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
  issueDeploymentEvidenceChallenge,
  loadCommittedMigrationClosure,
  loadDeploymentEvidenceVerifierChallenge,
  readDeploymentEvidenceReceipt,
} from "./deployment-evidence-receipt-io.mjs";
import {
  DEPLOYMENT_FIXTURE_ACTIVE_VERSION,
  DEPLOYMENT_FIXTURE_ARCHIVE,
  DEPLOYMENT_FIXTURE_COMMIT,
  DEPLOYMENT_FIXTURE_ID,
  DEPLOYMENT_FIXTURE_MIGRATIONS,
  DEPLOYMENT_FIXTURE_NOW,
  TEST_PRODUCER_IDENTITY,
  TEST_PRODUCER_KEY_ID,
  TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
  deploymentEvidenceChallenge,
  deploymentEvidenceEnvelope,
  deploymentEvidenceExpectation,
  deploymentEvidencePayload,
  deploymentEvidenceVerifierState,
  signDeploymentEvidencePayload,
  writeDeploymentEvidenceVerifierStateFixture,
} from "./deployment-evidence-test-fixture.js";

const REPO_ROOT = path.resolve(
  fileURLToPath(new URL("../..", import.meta.url)),
);

function resign(payload: Record<string, any>) {
  return signDeploymentEvidencePayload(payload);
}

function verify(payload: Record<string, any>, artifact: "A" | "B" = "B") {
  return verifyDeploymentEvidenceReceipt(
    resign(payload),
    deploymentEvidenceExpectation(artifact),
    {
      trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
      nowMs: DEPLOYMENT_FIXTURE_NOW,
    },
  );
}

function challengeRequest(artifact: "A" | "B" = "B") {
  return {
    archiveId: DEPLOYMENT_FIXTURE_ARCHIVE,
    artifact,
    artifactBundleSha256:
      deploymentEvidenceExpectation(artifact).artifactBundles[artifact],
    expectedCommit: DEPLOYMENT_FIXTURE_COMMIT,
    producerIdentity: TEST_PRODUCER_IDENTITY,
    producerKeyId: TEST_PRODUCER_KEY_ID,
  };
}

describe("producer-owned deployment evidence receipt v2", () => {
  it("admits nonempty exact Artifact B and Artifact A observation fixtures", () => {
    const finalReceipt = verify(
      deploymentEvidencePayload("B"),
      "B",
    );
    expect(finalReceipt).toMatchObject({
      producer_identity: TEST_PRODUCER_IDENTITY,
      payload: {
        artifact: "B",
        producer_sequence: 8,
        database: {
          migration_row_count: 2,
          schema_object_count: 3,
        },
        transition: {
          previous_artifact: "A",
          current_artifact: "B",
          permitted_transition: "artifact-a-to-artifact-b",
        },
        worker: {
          deployment_observation_field_count: 4,
          sender_filter_route: {
            item_count: 1,
            request_signature_byte_count: 37,
            response_field_count: 3,
          },
        },
        sender_filter: { advertised: true, version: 1 },
      },
    });
    expect(finalReceipt.receipt_sha256).toMatch(/^[1-9a-f][0-9a-f]{63}$/);

    const bridgeReceipt = verify(
      deploymentEvidencePayload("A"),
      "A",
    );
    expect(bridgeReceipt.payload).toMatchObject({
      artifact: "A",
      migrations: [{ applied_order: 1 }],
      transition: {
        previous_artifact: "legacy",
        permitted_transition: "legacy-to-artifact-a",
      },
      worker: {
        sender_filter_route: {
          status: 503,
          item_count: 0,
          response_field_count: 1,
        },
      },
      sender_filter: { advertised: false, version: null },
    });
  });

  it("keeps the production trust registry empty and refuses caller keys", () => {
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
        keyId: TEST_PRODUCER_KEY_ID,
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

  it("refuses every zeroed raw-observation or lineage digest", () => {
    const zeroCases: Array<
      [string, (payload: Record<string, any>) => void]
    > = [
      ["previous receipt", (p) => { p.previous_receipt_sha256 = "0".repeat(64); }],
      ["challenge receipt", (p) => { p.challenge.previous_receipt_sha256 = "0".repeat(64); }],
      ["archive", (p) => { p.archive_id = "0".repeat(64); }],
      ["artifact bundle", (p) => { p.artifact_bundle_sha256 = "0".repeat(64); }],
      ["challenge archive", (p) => { p.challenge.archive_id = "0".repeat(64); }],
      ["challenge bundle", (p) => { p.challenge.artifact_bundle_sha256 = "0".repeat(64); }],
      ["migration source", (p) => { p.migrations[0].sha256 = "0".repeat(64); }],
      ["migration query", (p) => { p.database.migration_list_query_sha256 = "0".repeat(64); }],
      ["migration output", (p) => { p.database.migration_list_output_sha256 = "0".repeat(64); }],
      ["schema query", (p) => { p.database.schema_query_sha256 = "0".repeat(64); }],
      ["schema output", (p) => { p.database.schema_output_sha256 = "0".repeat(64); }],
      ["schema fingerprint", (p) => { p.database.schema_fingerprint_sha256 = "0".repeat(64); }],
      ["Worker bundle", (p) => { p.worker.bundle_sha256 = "0".repeat(64); }],
      ["deployment observation", (p) => { p.worker.deployment_observation_sha256 = "0".repeat(64); }],
      ["health response", (p) => { p.worker.health_route.response_sha256 = "0".repeat(64); }],
      ["request signature", (p) => { p.worker.sender_filter_route.request_signature_sha256 = "0".repeat(64); }],
      ["sender response", (p) => { p.worker.sender_filter_route.response_sha256 = "0".repeat(64); }],
      ["capability response", (p) => { p.sender_filter.health_response_sha256 = "0".repeat(64); }],
      ["probe nonce", (p) => { p.artifact_isolation.probe_nonce_sha256 = "0".repeat(64); }],
      ["Artifact A bundle", (p) => { p.artifact_isolation.artifact_a.bundle_sha256 = "0".repeat(64); }],
      ["Artifact A probe", (p) => { p.artifact_isolation.artifact_a.probe_sha256 = "0".repeat(64); }],
      ["Artifact B bundle", (p) => { p.artifact_isolation.artifact_b.bundle_sha256 = "0".repeat(64); }],
      ["Artifact B probe", (p) => { p.artifact_isolation.artifact_b.probe_sha256 = "0".repeat(64); }],
    ];
    for (const [_name, mutate] of zeroCases) {
      const payload = deploymentEvidencePayload();
      mutate(payload);
      expect(() => verify(payload)).toThrow();
    }
    const zeroPayloadDigest = deploymentEvidenceEnvelope();
    zeroPayloadDigest.payload_sha256 = "0".repeat(64);
    expect(() =>
      verifyDeploymentEvidenceReceipt(
        zeroPayloadDigest,
        deploymentEvidenceExpectation(),
        {
          trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
          nowMs: DEPLOYMENT_FIXTURE_NOW,
        },
      ),
    ).toThrow(/nonzero lowercase SHA-256/);
  });

  it("recomputes all raw observation hashes and cardinalities", () => {
    const cases: Array<
      [string, (payload: Record<string, any>) => void, RegExp]
    > = [
      [
        "migration cardinality",
        (p) => { p.database.migration_row_count = 0; },
        /migration row cardinality/,
      ],
      [
        "migration raw bytes",
        (p) => { p.database.migration_rows[0].applied_at += " "; },
        /migration output digest mismatch/,
      ],
      [
        "schema bytes",
        (p) => { p.database.schema_rows[0].sql += " "; },
        /schema observation digest mismatch/,
      ],
      [
        "schema cardinality",
        (p) => { p.database.schema_object_count += 1; },
        /schema object cardinality mismatch/,
      ],
      [
        "deployment raw fields",
        (p) => { p.worker.deployment_observation.service = "other"; },
        /deployment observation digest mismatch/,
      ],
      [
        "health bytes",
        (p) => { p.worker.health_route.response.ok = false; },
        /health response observation digest mismatch/,
      ],
      [
        "health cardinality",
        (p) => { p.worker.health_route.response_field_count = 0; },
        /raw cardinality/,
      ],
      [
        "deployment cardinality",
        (p) => { p.worker.deployment_observation_field_count = 0; },
        /raw cardinality/,
      ],
      [
        "signature cardinality",
        (p) => { p.worker.sender_filter_route.request_signature_byte_count = 0; },
        /signature cardinality/,
      ],
      [
        "sender response cardinality",
        (p) => { p.worker.sender_filter_route.response_field_count = 0; },
        /raw cardinality/,
      ],
      [
        "signature bytes",
        (p) => { p.worker.sender_filter_route.request_signature_b64 = "AQ=="; },
        /request observation mismatch/,
      ],
      [
        "sender bytes",
        (p) => { p.worker.sender_filter_route.response.items[0].sender_id = "other"; },
        /response observation digest mismatch/,
      ],
      [
        "probe bytes",
        (p) => { p.artifact_isolation.artifact_a.observation.active = true; },
        /probe observation digest mismatch/,
      ],
      [
        "nonce bytes",
        (p) => { p.artifact_isolation.probe_nonce_b64 = "AQ=="; },
        /nonce observation mismatch/,
      ],
      [
        "nonce cardinality",
        (p) => { p.artifact_isolation.probe_nonce_byte_count = 0; },
        /nonce cardinality/,
      ],
      [
        "Artifact A probe cardinality",
        (p) => { p.artifact_isolation.artifact_a.observation_field_count = 0; },
        /raw cardinality/,
      ],
      [
        "Artifact B probe cardinality",
        (p) => { p.artifact_isolation.artifact_b.observation_field_count = 0; },
        /raw cardinality/,
      ],
    ];
    for (const [_name, mutate, error] of cases) {
      const payload = deploymentEvidencePayload();
      mutate(payload);
      expect(() => verify(payload)).toThrow(error);
    }
  });

  it("refuses a producer-chosen or forged challenge even when re-signed", () => {
    const forgedNonce = deploymentEvidencePayload();
    forgedNonce.challenge.nonce_b64 = Buffer.alloc(32, 0x41).toString("base64");
    expect(() => verify(forgedNonce)).toThrow(
      /does not carry the verifier-issued challenge/,
    );

    const forgedId = deploymentEvidencePayload();
    forgedId.challenge.challenge_id =
      "cccccccc-dddd-4eee-8fff-000000000000";
    expect(() => verify(forgedId)).toThrow(
      /does not carry the verifier-issued challenge/,
    );
  });

  it("refuses a caller-forged challenge even if pure inputs agree", async () => {
    const testStateDirectory = await mkdtemp(
      path.join(tmpdir(), "deployment-verifier-forged-challenge-"),
    );
    await writeDeploymentEvidenceVerifierStateFixture(testStateDirectory);
    const payload = deploymentEvidencePayload();
    payload.challenge.nonce_b64 =
      Buffer.alloc(32, 0x41).toString("base64");
    const expectation = deploymentEvidenceExpectation();
    expectation.verifierChallenge = structuredClone(payload.challenge);
    const purelyVerified = verifyDeploymentEvidenceReceipt(
      resign(payload),
      expectation,
      {
        trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      },
    );
    await expect(
      consumeDeploymentEvidenceOnce(purelyVerified, { testStateDirectory }),
    ).rejects.toThrow(/challenge is absent, forged, stale, or replayed/);
  });

  it("binds exact predecessor Worker, deployment, and permitted transition", () => {
    const wrongPreviousWorker = deploymentEvidencePayload();
    wrongPreviousWorker.transition.previous_worker_version =
      "99999999-8888-4777-8666-555555555555";
    expect(() => verify(wrongPreviousWorker)).toThrow(
      /transition does not match verifier state/,
    );

    const wrongPreviousDeployment = deploymentEvidencePayload();
    wrongPreviousDeployment.transition.previous_deployment_id =
      "99999999-8888-4777-8666-555555555555";
    expect(() => verify(wrongPreviousDeployment)).toThrow(
      /transition does not match verifier state/,
    );

    const wrongTransition = deploymentEvidencePayload();
    wrongTransition.challenge.permitted_transition = "artifact-b-forward";
    wrongTransition.transition.permitted_transition = "artifact-b-forward";
    const expectation = deploymentEvidenceExpectation();
    expectation.verifierChallenge = structuredClone(wrongTransition.challenge);
    expect(() =>
      verifyDeploymentEvidenceReceipt(resign(wrongTransition), expectation, {
        trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      }),
    ).toThrow(/transition is not permitted/);
  });

  it("retains exact commit, archive, D1, migration, Worker, route, and A/B bindings", () => {
    const cases: Array<
      [string, (payload: Record<string, any>) => void, RegExp]
    > = [
      [
        "commit",
        (p) => { p.expected_commit = "f".repeat(40); },
        /commit, archive, artifact, or bundle mismatch/,
      ],
      [
        "archive",
        (p) => { p.archive_id = "e".repeat(64); },
        /commit, archive, artifact, or bundle mismatch/,
      ],
      [
        "database",
        (p) => { p.database.id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"; },
        /database identity mismatch/,
      ],
      [
        "migration order",
        (p) => { p.migrations.reverse(); },
        /migration order or digest mismatch/,
      ],
      [
        "Worker version",
        (p) => {
          p.worker.version_id =
            "99999999-8888-4777-8666-555555555555";
        },
        /Worker identity or bundle mismatch/,
      ],
      [
        "route",
        (p) => { p.worker.sender_filter_route.path = "/v1/control-inbox"; },
        /sender-filter route contract mismatch/,
      ],
      [
        "capability",
        (p) => { p.sender_filter.version = 2; },
        /capability advertisement mismatch/,
      ],
      [
        "Artifact isolation",
        (p) => {
          p.artifact_isolation.artifact_a.active = true;
          p.artifact_isolation.artifact_a.observed_version_id =
            DEPLOYMENT_FIXTURE_ACTIVE_VERSION;
        },
        /probe observation mismatch|Artifact A\/B isolation mismatch/,
      ],
    ];
    for (const [_name, mutate, error] of cases) {
      const payload = deploymentEvidencePayload();
      mutate(payload);
      expect(() => verify(payload)).toThrow(error);
    }
  });

  it("issues a nonce only from initialized durable verifier state", async () => {
    const testStateDirectory = await mkdtemp(
      path.join(tmpdir(), "deployment-verifier-challenge-"),
    );
    await writeDeploymentEvidenceVerifierStateFixture(
      testStateDirectory,
      "B",
      { pending_challenge: null },
    );
    const challenge = await issueDeploymentEvidenceChallenge(
      challengeRequest(),
      {
        testStateDirectory,
        nowMs: Date.parse("2026-07-27T11:59:45.000Z"),
        randomBytesFn: () => Buffer.alloc(32, 0x7a),
        randomUuidFn: () =>
          "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff",
      },
    );
    expect(challenge).toEqual(deploymentEvidenceChallenge());
    await expect(
      loadDeploymentEvidenceVerifierChallenge(TEST_PRODUCER_KEY_ID, {
        testStateDirectory,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      }),
    ).resolves.toEqual(challenge);
    await expect(
      issueDeploymentEvidenceChallenge(challengeRequest(), {
        testStateDirectory,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      }),
    ).rejects.toThrow(/already has a pending challenge/);

    const emptyStateDirectory = await mkdtemp(
      path.join(tmpdir(), "deployment-verifier-no-genesis-"),
    );
    await expect(
      issueDeploymentEvidenceChallenge(challengeRequest(), {
        testStateDirectory: emptyStateDirectory,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      }),
    ).rejects.toThrow(/state is missing; genesis is forbidden/);
  });

  it("consumes once and refuses replay or deleted-ledger genesis reset", async () => {
    const testStateDirectory = await mkdtemp(
      path.join(tmpdir(), "deployment-verifier-consume-"),
    );
    const { statePath } =
      await writeDeploymentEvidenceVerifierStateFixture(testStateDirectory);
    const verified = verify(deploymentEvidencePayload());
    await expect(
      consumeDeploymentEvidenceOnce(verified, { testStateDirectory }),
    ).resolves.toMatchObject({
      sequence: 8,
      current_worker_version: DEPLOYMENT_FIXTURE_ACTIVE_VERSION,
      current_deployment_id: DEPLOYMENT_FIXTURE_ID,
      pending_challenge: null,
    });
    await expect(
      consumeDeploymentEvidenceOnce(verified, { testStateDirectory }),
    ).rejects.toThrow(/challenge is absent, forged, stale, or replayed/);

    await rename(statePath, `${statePath}.deleted-for-test`);
    await expect(
      consumeDeploymentEvidenceOnce(verified, { testStateDirectory }),
    ).rejects.toThrow(/state is missing; genesis is forbidden/);

    const freshEmptyDirectory = await mkdtemp(
      path.join(tmpdir(), "deployment-verifier-reset-"),
    );
    await expect(
      consumeDeploymentEvidenceOnce(verified, {
        testStateDirectory: freshEmptyDirectory,
      }),
    ).rejects.toThrow(/state is missing; genesis is forbidden/);
  });

  it("refuses a Worker rollback recorded anywhere in durable history", async () => {
    const testStateDirectory = await mkdtemp(
      path.join(tmpdir(), "deployment-verifier-rollback-"),
    );
    const base = deploymentEvidenceVerifierState();
    await writeDeploymentEvidenceVerifierStateFixture(
      testStateDirectory,
      "B",
      {
        seen_worker_versions: [
          ...(base.seen_worker_versions as string[]),
          DEPLOYMENT_FIXTURE_ACTIVE_VERSION,
        ],
        seen_deployment_ids: [
          ...(base.seen_deployment_ids as string[]),
          DEPLOYMENT_FIXTURE_ID,
        ],
      },
    );
    const verified = verify(deploymentEvidencePayload());
    await expect(
      consumeDeploymentEvidenceOnce(verified, { testStateDirectory }),
    ).rejects.toThrow(/Worker rollback or replay refused/);

    const downgradeDirectory = await mkdtemp(
      path.join(tmpdir(), "deployment-verifier-downgrade-"),
    );
    await writeDeploymentEvidenceVerifierStateFixture(
      downgradeDirectory,
      "B",
      {
        current_artifact: "B",
        pending_challenge: null,
      },
    );
    await expect(
      issueDeploymentEvidenceChallenge(challengeRequest("A"), {
        testStateDirectory: downgradeDirectory,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      }),
    ).rejects.toThrow(/transition is not permitted/);
  });

  it("refuses stale, reversed, and overlong receipt or challenge times", () => {
    const cases: Array<
      [(payload: Record<string, any>) => void, RegExp]
    > = [
      [
        (p) => {
          p.timestamps = {
            action_started_at: "2026-07-27T11:54:50.000Z",
            migrations_captured_at: "2026-07-27T11:54:52.000Z",
            worker_deployed_at: "2026-07-27T11:54:54.000Z",
            probes_finished_at: "2026-07-27T11:54:57.000Z",
            issued_at: "2026-07-27T11:54:58.000Z",
          };
          p.challenge.issued_at = "2026-07-27T11:54:45.000Z";
          p.challenge.expires_at = "2026-07-27T12:04:45.000Z";
          const expectation = deploymentEvidenceExpectation();
          expectation.verifierChallenge = structuredClone(p.challenge);
          p.__expectation = expectation;
        },
        /stale or future-dated/,
      ],
      [
        (p) => { p.timestamps.worker_deployed_at = "2026-07-27T11:59:49.000Z"; },
        /timestamps are out of order/,
      ],
      [
        (p) => { p.timestamps.action_started_at = "2026-07-27T11:40:00.000Z"; },
        /action interval is too long|challenge does not cover/,
      ],
      [
        (p) => {
          p.challenge.expires_at = "2026-07-27T11:59:56.000Z";
          const expectation = deploymentEvidenceExpectation();
          expectation.verifierChallenge = structuredClone(p.challenge);
          p.__expectation = expectation;
        },
        /challenge does not cover/,
      ],
    ];
    for (const [mutate, error] of cases) {
      const payload = deploymentEvidencePayload();
      mutate(payload);
      const expectation =
        payload.__expectation ?? deploymentEvidenceExpectation();
      delete payload.__expectation;
      expect(() =>
        verifyDeploymentEvidenceReceipt(resign(payload), expectation, {
          trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
          nowMs: DEPLOYMENT_FIXTURE_NOW,
        }),
      ).toThrow(error);
    }
  });

  it("reads only a nonempty absolute regular receipt file", async () => {
    const directory = await mkdtemp(
      path.join(tmpdir(), "deployment-evidence-file-"),
    );
    const receiptPath = path.join(directory, "receipt.json");
    await writeFile(receiptPath, JSON.stringify(deploymentEvidenceEnvelope()));
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
      migrations.every(
        (entry) =>
          /^[0-9a-f]{64}$/.test(entry.sha256) &&
          entry.sha256 !== "0".repeat(64),
      ),
    ).toBe(true);
    expect(new Set(migrations.map((entry) => entry.sha256)).size).toBe(
      migrations.length,
    );
  });
});
