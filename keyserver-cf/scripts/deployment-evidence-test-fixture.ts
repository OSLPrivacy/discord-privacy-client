import {
  generateKeyPairSync,
  sign,
  type KeyObject,
} from "node:crypto";
import {
  DEPLOYMENT_EVIDENCE_ENVELOPE_FORMAT,
  DEPLOYMENT_EVIDENCE_PAYLOAD_FORMAT,
  DEPLOYMENT_MIGRATION_LIST_QUERY,
  DEPLOYMENT_SCHEMA_FINGERPRINT_QUERY,
  deploymentEvidenceSigningBytes,
} from "./deployment-evidence-receipt-contract.mjs";
import { canonicalJson, sha256 } from "./readiness-artifact-contract.mjs";

export const DEPLOYMENT_FIXTURE_NOW =
  Date.parse("2026-07-27T12:00:00.000Z");
export const DEPLOYMENT_FIXTURE_COMMIT = "a".repeat(40);
export const DEPLOYMENT_FIXTURE_ARCHIVE = "d".repeat(64);
export const DEPLOYMENT_FIXTURE_ACTIVE_VERSION =
  "11111111-2222-4333-8444-555555555555";
export const DEPLOYMENT_FIXTURE_ID =
  "66666666-7777-4888-9999-aaaaaaaaaaaa";
export const DEPLOYMENT_FIXTURE_BUNDLES = {
  A: "3".repeat(64),
  B: "4".repeat(64),
};
export const DEPLOYMENT_FIXTURE_MIGRATIONS = [
  {
    name: "0030_reserve_derived_identity_namespace.sql",
    sha256: "5".repeat(64),
  },
  {
    name: "0031_control_inbox_sender_retention.sql",
    sha256: "6".repeat(64),
  },
];

const TEST_PRODUCER_KEY_ID = "test-independent-release-producer";
const TEST_PRODUCER_IDENTITY = "test://independent-release-producer";
const testPair = generateKeyPairSync("ed25519");

export const TEST_TRUSTED_DEPLOYMENT_PRODUCERS = {
  [TEST_PRODUCER_KEY_ID]: {
    identity: TEST_PRODUCER_IDENTITY,
    public_key_spki_b64: testPair.publicKey
      .export({ format: "der", type: "spki" })
      .toString("base64"),
  },
};

export function deploymentEvidenceExpectation(
  artifact: "A" | "B" = "B",
) {
  return {
    archiveId: DEPLOYMENT_FIXTURE_ARCHIVE,
    artifact,
    artifactBundles: DEPLOYMENT_FIXTURE_BUNDLES,
    expectedCommit: DEPLOYMENT_FIXTURE_COMMIT,
    expectedDeploymentId: DEPLOYMENT_FIXTURE_ID,
    expectedMigrations: DEPLOYMENT_FIXTURE_MIGRATIONS,
    expectedWorkerVersion: DEPLOYMENT_FIXTURE_ACTIVE_VERSION,
  };
}

export function deploymentEvidencePayload(
  artifact: "A" | "B" = "B",
): Record<string, any> {
  const selectedBundle = DEPLOYMENT_FIXTURE_BUNDLES[artifact];
  const activeA = artifact === "A";
  const migrations = DEPLOYMENT_FIXTURE_MIGRATIONS
    .slice(0, artifact === "A" ? 1 : 2)
    .map((entry, index) => ({
      applied_order: index + 1,
      ...entry,
    }));
  return {
    format: DEPLOYMENT_EVIDENCE_PAYLOAD_FORMAT,
    producer_identity: TEST_PRODUCER_IDENTITY,
    producer_run_id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
    producer_sequence: 1,
    previous_receipt_sha256: "0".repeat(64),
    authorization_id: "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff",
    expected_commit: DEPLOYMENT_FIXTURE_COMMIT,
    archive_id: DEPLOYMENT_FIXTURE_ARCHIVE,
    artifact,
    artifact_bundle_sha256: selectedBundle,
    database: {
      name: "osl-keyserver-prod",
      id: "1de837cd-3bf6-4d33-be82-12d358523600",
      environment: "production",
      migration_list_query_sha256: sha256(
        Buffer.from(DEPLOYMENT_MIGRATION_LIST_QUERY),
      ),
      migration_list_output_sha256: sha256(
        Buffer.from(canonicalJson(migrations)),
      ),
      schema_query_sha256: sha256(
        Buffer.from(DEPLOYMENT_SCHEMA_FINGERPRINT_QUERY),
      ),
      schema_output_sha256: "9".repeat(64),
      schema_fingerprint_sha256: "9".repeat(64),
      schema_object_count: 17,
    },
    migrations,
    worker: {
      service: "oslprivacy-keyserver",
      version_id: DEPLOYMENT_FIXTURE_ACTIVE_VERSION,
      deployment_id: DEPLOYMENT_FIXTURE_ID,
      bundle_sha256: selectedBundle,
      health_route: {
        method: "GET",
        path: "/v1/healthz",
        status: 200,
        response_sha256: "a".repeat(64),
      },
      sender_filter_route: {
        method: "GET",
        path: "/v1/control-inbox/:user_id",
        status: artifact === "B" ? 200 : 503,
        request_signature_sha256: "b".repeat(64),
        response_sha256: "c".repeat(64),
        filtered_sender_id: artifact === "B" ? "sender-positive" : null,
        item_count: artifact === "B" ? 1 : 0,
      },
    },
    sender_filter: {
      name: "control_inbox_sender_disposition",
      advertised: artifact === "B",
      version: artifact === "B" ? 1 : null,
      probe_sender_id: artifact === "B" ? "sender-positive" : null,
      health_response_sha256: "a".repeat(64),
    },
    artifact_isolation: {
      probe_nonce_sha256: "d".repeat(64),
      artifact_a: {
        bundle_sha256: DEPLOYMENT_FIXTURE_BUNDLES.A,
        active: activeA,
        observed_version_id:
          activeA ? DEPLOYMENT_FIXTURE_ACTIVE_VERSION : null,
        probe_sha256: "e".repeat(64),
      },
      artifact_b: {
        bundle_sha256: DEPLOYMENT_FIXTURE_BUNDLES.B,
        active: !activeA,
        observed_version_id:
          activeA ? null : DEPLOYMENT_FIXTURE_ACTIVE_VERSION,
        probe_sha256: "f".repeat(64),
      },
    },
    timestamps: {
      action_started_at: "2026-07-27T11:59:50.000Z",
      migrations_captured_at: "2026-07-27T11:59:52.000Z",
      worker_deployed_at: "2026-07-27T11:59:54.000Z",
      probes_finished_at: "2026-07-27T11:59:57.000Z",
      issued_at: "2026-07-27T11:59:58.000Z",
    },
  };
}

export function signDeploymentEvidencePayload(
  payload: Record<string, unknown>,
  {
    keyId = TEST_PRODUCER_KEY_ID,
    privateKey = testPair.privateKey,
  }: { keyId?: string; privateKey?: KeyObject } = {},
) {
  const payloadSha256 = sha256(Buffer.from(canonicalJson(payload)));
  return {
    format: DEPLOYMENT_EVIDENCE_ENVELOPE_FORMAT,
    producer_key_id: keyId,
    payload,
    payload_sha256: payloadSha256,
    signature_b64: sign(
      null,
      deploymentEvidenceSigningBytes(payload),
      privateKey,
    ).toString("base64"),
  };
}

export function deploymentEvidenceEnvelope(
  artifact: "A" | "B" = "B",
) {
  return signDeploymentEvidencePayload(deploymentEvidencePayload(artifact));
}
