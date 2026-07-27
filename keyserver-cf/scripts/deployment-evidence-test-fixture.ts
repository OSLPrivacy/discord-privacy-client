import {
  generateKeyPairSync,
  sign,
  type KeyObject,
} from "node:crypto";
import {
  DEPLOYMENT_EVIDENCE_CHALLENGE_FORMAT,
  DEPLOYMENT_EVIDENCE_ENVELOPE_FORMAT,
  DEPLOYMENT_EVIDENCE_PAYLOAD_FORMAT,
  DEPLOYMENT_MIGRATION_LIST_QUERY,
  DEPLOYMENT_REGISTERED_SIGNER_QUERY,
  DEPLOYMENT_SCHEMA_FINGERPRINT_QUERY,
  deploymentEvidenceSenderFilterCanonicalBytes,
  deploymentEvidenceSigningBytes,
} from "./deployment-evidence-receipt-contract.mjs";
import {
  DEPLOYMENT_EVIDENCE_VERIFIER_STATE_FORMAT,
} from "./deployment-evidence-receipt-io.mjs";
import {
  DEPLOYMENT_EVIDENCE_VERIFIER_ADMINISTRATION_DOMAIN,
  DEPLOYMENT_EVIDENCE_VERIFIER_STORE_FORMAT,
} from "./deployment-evidence-verifier-store.mjs";
import { canonicalJson, sha256 } from "./readiness-artifact-contract.mjs";

export const DEPLOYMENT_FIXTURE_NOW =
  Date.parse("2026-07-27T12:00:00.000Z");
export const DEPLOYMENT_FIXTURE_COMMIT = "a".repeat(40);
export const DEPLOYMENT_FIXTURE_ARCHIVE = "d".repeat(64);
export const DEPLOYMENT_FIXTURE_ACTIVE_VERSION =
  "11111111-2222-4333-8444-555555555555";
export const DEPLOYMENT_FIXTURE_ID =
  "66666666-7777-4888-9999-aaaaaaaaaaaa";
export const DEPLOYMENT_FIXTURE_PREVIOUS_VERSION =
  "77777777-2222-4333-8444-555555555555";
export const DEPLOYMENT_FIXTURE_PREVIOUS_ID =
  "88888888-7777-4888-9999-bbbbbbbbbbbb";
export const DEPLOYMENT_FIXTURE_PREVIOUS_RECEIPT = "8".repeat(64);
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
  {
    name: "0032_sender_filter_capability_floor.sql",
    sha256: "7".repeat(64),
  },
  {
    name: "0033_canonical_identity_rollout_authority.sql",
    sha256: "8".repeat(64),
  },
];

export const TEST_PRODUCER_KEY_ID =
  "test-independent-release-producer";
export const TEST_PRODUCER_IDENTITY =
  "test://independent-release-producer";
const TEST_CHALLENGE_ID = "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff";
const TEST_CHALLENGE_NONCE = Buffer.alloc(32, 0x7a);
const TEST_PROBE_NONCE = Buffer.from(
  "artifact-isolation-positive-nonce",
  "utf8",
);
const testPair = generateKeyPairSync("ed25519");
const testRequesterPair = generateKeyPairSync("ed25519");
const TEST_REQUEST_USER_ID = "recipient-positive";
const TEST_REQUEST_SENDER_ID = "sender-positive";
const TEST_REQUEST_TIMESTAMP_MS = Date.parse(
  "2026-07-27T11:59:56.000Z",
);
const TEST_REQUEST_CANONICAL =
  deploymentEvidenceSenderFilterCanonicalBytes({
    requestUserId: TEST_REQUEST_USER_ID,
    requestTimestampMs: TEST_REQUEST_TIMESTAMP_MS,
    requestSenderId: TEST_REQUEST_SENDER_ID,
  });
const TEST_REQUEST_SIGNATURE = sign(
  null,
  TEST_REQUEST_CANONICAL,
  testRequesterPair.privateKey,
);
const TEST_REQUESTER_PUBLIC_KEY = Buffer.from(
  testRequesterPair.publicKey.export({ format: "der", type: "spki" }),
).subarray(-32);

export const TEST_TRUSTED_DEPLOYMENT_PRODUCERS = {
  [TEST_PRODUCER_KEY_ID]: {
    identity: TEST_PRODUCER_IDENTITY,
    public_key_spki_b64: testPair.publicKey
      .export({ format: "der", type: "spki" })
      .toString("base64"),
  },
};

function digest(value: unknown) {
  return sha256(Buffer.from(canonicalJson(value)));
}

function fixturePredecessor(artifact: "A" | "B") {
  return {
    previousArtifact: artifact === "A" ? "legacy" : "A",
    permittedTransition:
      artifact === "A"
        ? "legacy-to-artifact-a"
        : "artifact-a-to-artifact-b",
  };
}

export function deploymentEvidenceChallenge(
  artifact: "A" | "B" = "B",
) {
  const predecessor = fixturePredecessor(artifact);
  return {
    format: DEPLOYMENT_EVIDENCE_CHALLENGE_FORMAT,
    challenge_id: TEST_CHALLENGE_ID,
    nonce_b64: TEST_CHALLENGE_NONCE.toString("base64"),
    issued_at: "2026-07-27T11:59:45.000Z",
    expires_at: "2026-07-27T12:09:45.000Z",
    producer_key_id: TEST_PRODUCER_KEY_ID,
    producer_identity: TEST_PRODUCER_IDENTITY,
    previous_sequence: 7,
    previous_receipt_sha256: DEPLOYMENT_FIXTURE_PREVIOUS_RECEIPT,
    previous_worker_version: DEPLOYMENT_FIXTURE_PREVIOUS_VERSION,
    previous_deployment_id: DEPLOYMENT_FIXTURE_PREVIOUS_ID,
    previous_artifact: predecessor.previousArtifact,
    permitted_transition: predecessor.permittedTransition,
    expected_commit: DEPLOYMENT_FIXTURE_COMMIT,
    archive_id: DEPLOYMENT_FIXTURE_ARCHIVE,
    artifact,
    artifact_bundle_sha256: DEPLOYMENT_FIXTURE_BUNDLES[artifact],
  };
}

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
    verifierChallenge: deploymentEvidenceChallenge(artifact),
  };
}

export function deploymentEvidenceVerifierState(
  artifact: "A" | "B" = "B",
  overrides: Record<string, unknown> = {},
) {
  const challenge = deploymentEvidenceChallenge(artifact);
  return {
    format: DEPLOYMENT_EVIDENCE_VERIFIER_STATE_FORMAT,
    producer_key_id: TEST_PRODUCER_KEY_ID,
    producer_identity: TEST_PRODUCER_IDENTITY,
    state_epoch: 3,
    sequence: challenge.previous_sequence,
    receipt_sha256: challenge.previous_receipt_sha256,
    last_producer_run_id: "99999999-aaaa-4bbb-8ccc-dddddddddddd",
    current_artifact: challenge.previous_artifact,
    current_worker_version: challenge.previous_worker_version,
    current_deployment_id: challenge.previous_deployment_id,
    seen_worker_versions: [challenge.previous_worker_version],
    seen_deployment_ids: [challenge.previous_deployment_id],
    pending_challenge: challenge,
    ...overrides,
  };
}

export function createDeploymentEvidenceVerifierStoreFixture(
  artifact: "A" | "B" = "B",
  overrides: Record<string, unknown> = {},
  controls: { missing?: boolean; conflict?: boolean } = {},
) {
  const initialState = deploymentEvidenceVerifierState(artifact, overrides);
  let snapshot: {
    monotonic_version: number;
    state: Record<string, any>;
  } | null = controls.missing
    ? null
    : {
        monotonic_version: Number(initialState.state_epoch),
        state: initialState,
      };
  let casCalls = 0;
  const store = {
    format: DEPLOYMENT_EVIDENCE_VERIFIER_STORE_FORMAT,
    provisioned: true,
    administration_domain:
      DEPLOYMENT_EVIDENCE_VERIFIER_ADMINISTRATION_DOMAIN,
    administrator_identity: "test://independent-verifier-administrator",
    database_id: "12345678-1234-4abc-8def-123456789abc",
    environment: "production",
    async readCurrent(producerKeyId: string) {
      if (
        snapshot === null ||
        snapshot.state.producer_key_id !== producerKeyId
      ) {
        return null;
      }
      return structuredClone(snapshot);
    },
    async compareAndSwap(input: {
      expected_monotonic_version: number;
      next_monotonic_version: number;
      next_state: Record<string, any>;
      producer_key_id: string;
    }) {
      casCalls += 1;
      if (
        controls.conflict ||
        snapshot === null ||
        snapshot.state.producer_key_id !== input.producer_key_id ||
        snapshot.monotonic_version !== input.expected_monotonic_version
      ) {
        return {
          applied: false,
          observed_monotonic_version:
            snapshot?.monotonic_version ??
            input.expected_monotonic_version,
        };
      }
      snapshot = {
        monotonic_version: input.next_monotonic_version,
        state: structuredClone(input.next_state),
      };
      return {
        applied: true,
        observed_monotonic_version: input.next_monotonic_version,
      };
    },
  };
  return {
    store,
    current: () => structuredClone(snapshot),
    casCalls: () => casCalls,
  };
}

function artifactProbe(
  active: boolean,
  bundleSha256: string,
  observedVersionId: string | null,
) {
  const observation = {
    active,
    bundle_sha256: bundleSha256,
    observed_version_id: observedVersionId,
  };
  return {
    ...observation,
    observation,
    observation_field_count: Object.keys(observation).length,
    probe_sha256: digest(observation),
  };
}

export function deploymentEvidencePayload(
  artifact: "A" | "B" = "B",
): Record<string, any> {
  const challenge = deploymentEvidenceChallenge(artifact);
  const selectedBundle = DEPLOYMENT_FIXTURE_BUNDLES[artifact];
  const activeA = artifact === "A";
  const migrations = DEPLOYMENT_FIXTURE_MIGRATIONS
    .slice(0, artifact === "A" ? 1 : DEPLOYMENT_FIXTURE_MIGRATIONS.length)
    .map((entry, index) => ({
      applied_order: index + 1,
      ...entry,
    }));
  const schemaRows = [
    {
      type: "index",
      name: "idx_control_inbox_sender",
      tbl_name: "control_inbox",
      sql: "CREATE INDEX idx_control_inbox_sender ON control_inbox(sender_id)",
    },
    {
      type: "table",
      name: "control_inbox",
      tbl_name: "control_inbox",
      sql: "CREATE TABLE control_inbox (id TEXT, sender_id TEXT)",
    },
    {
      type: "table",
      name: "d1_migrations",
      tbl_name: "d1_migrations",
      sql: "CREATE TABLE d1_migrations (id INTEGER, name TEXT)",
    },
  ];
  const migrationRows = migrations.map((entry, index) => ({
    applied_at: `2026-07-${String(index + 1).padStart(2, "0")}T00:00:00.000Z`,
    id: index + 30,
    name: entry.name,
  }));
  const healthResponse =
    artifact === "B"
      ? {
          capabilities: {
            control_inbox_sender_disposition: 1,
          },
          ok: true,
        }
      : { capabilities: {}, ok: true };
  const senderResponse =
    artifact === "B"
      ? {
          filtered_sender_delivery: {
            live: 1,
            quarantined: 0,
            retired: 0,
            retryable: 0,
          },
          filtered_sender_id: "sender-positive",
          items: [
            {
              bundle_b64: Buffer.from("nonempty-bundle").toString("base64"),
              sender_id: "sender-positive",
            },
          ],
        }
      : { error: "sender filter unavailable before migration 0031" };
  const deploymentObservation = {
    bundle_sha256: selectedBundle,
    deployment_id: DEPLOYMENT_FIXTURE_ID,
    service: "oslprivacy-keyserver",
    version_id: DEPLOYMENT_FIXTURE_ACTIVE_VERSION,
  };
  const registeredSignerObservation = {
    identity_lookup_enabled: 1,
    ik_ed25519_pub_b64: TEST_REQUESTER_PUBLIC_KEY.toString("base64"),
    user_id: TEST_REQUEST_USER_ID,
  };
  return {
    format: DEPLOYMENT_EVIDENCE_PAYLOAD_FORMAT,
    producer_identity: TEST_PRODUCER_IDENTITY,
    producer_run_id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
    producer_sequence: challenge.previous_sequence + 1,
    previous_receipt_sha256: challenge.previous_receipt_sha256,
    challenge,
    transition: {
      previous_artifact: challenge.previous_artifact,
      previous_worker_version: challenge.previous_worker_version,
      previous_deployment_id: challenge.previous_deployment_id,
      current_artifact: artifact,
      current_worker_version: DEPLOYMENT_FIXTURE_ACTIVE_VERSION,
      current_deployment_id: DEPLOYMENT_FIXTURE_ID,
      permitted_transition: challenge.permitted_transition,
    },
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
      migration_rows: migrationRows,
      migration_list_output_sha256: digest(migrationRows),
      migration_row_count: migrationRows.length,
      schema_query_sha256: sha256(
        Buffer.from(DEPLOYMENT_SCHEMA_FINGERPRINT_QUERY),
      ),
      schema_rows: schemaRows,
      schema_output_sha256: digest(schemaRows),
      schema_fingerprint_sha256: digest(schemaRows),
      schema_object_count: schemaRows.length,
    },
    migrations,
    worker: {
      service: "oslprivacy-keyserver",
      version_id: DEPLOYMENT_FIXTURE_ACTIVE_VERSION,
      deployment_id: DEPLOYMENT_FIXTURE_ID,
      bundle_sha256: selectedBundle,
      deployment_observation: deploymentObservation,
      deployment_observation_field_count:
        Object.keys(deploymentObservation).length,
      deployment_observation_sha256: digest(deploymentObservation),
      health_route: {
        method: "GET",
        path: "/v1/healthz",
        status: 200,
        response: healthResponse,
        response_field_count: Object.keys(healthResponse).length,
        response_sha256: digest(healthResponse),
      },
      sender_filter_route: {
        method: "GET",
        path: "/v1/control-inbox/:user_id",
        status: artifact === "B" ? 200 : 503,
        registered_signer: {
          lookup_query_sha256: sha256(
            Buffer.from(DEPLOYMENT_REGISTERED_SIGNER_QUERY),
          ),
          observation: registeredSignerObservation,
          observation_field_count:
            Object.keys(registeredSignerObservation).length,
          observation_sha256: digest(registeredSignerObservation),
        },
        request_canonical_sha256: sha256(TEST_REQUEST_CANONICAL),
        request_sender_id: TEST_REQUEST_SENDER_ID,
        request_signature_b64: TEST_REQUEST_SIGNATURE.toString("base64"),
        request_signature_byte_count: TEST_REQUEST_SIGNATURE.byteLength,
        request_signature_sha256: sha256(TEST_REQUEST_SIGNATURE),
        request_timestamp_ms: TEST_REQUEST_TIMESTAMP_MS,
        request_user_id: TEST_REQUEST_USER_ID,
        response: senderResponse,
        response_field_count: Object.keys(senderResponse).length,
        response_sha256: digest(senderResponse),
        filtered_sender_id: artifact === "B" ? "sender-positive" : null,
        item_count: artifact === "B" ? 1 : 0,
      },
    },
    sender_filter: {
      name: "control_inbox_sender_disposition",
      advertised: artifact === "B",
      version: artifact === "B" ? 1 : null,
      probe_sender_id: artifact === "B" ? "sender-positive" : null,
      health_response_sha256: digest(healthResponse),
    },
    artifact_isolation: {
      probe_nonce_b64: TEST_PROBE_NONCE.toString("base64"),
      probe_nonce_byte_count: TEST_PROBE_NONCE.byteLength,
      probe_nonce_sha256: sha256(TEST_PROBE_NONCE),
      artifact_a: artifactProbe(
        activeA,
        DEPLOYMENT_FIXTURE_BUNDLES.A,
        activeA ? DEPLOYMENT_FIXTURE_ACTIVE_VERSION : null,
      ),
      artifact_b: artifactProbe(
        !activeA,
        DEPLOYMENT_FIXTURE_BUNDLES.B,
        activeA ? null : DEPLOYMENT_FIXTURE_ACTIVE_VERSION,
      ),
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
  const payloadSha256 = digest(payload);
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
