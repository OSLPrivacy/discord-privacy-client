import { createPublicKey, verify as verifySignature } from "node:crypto";
import {
  READINESS_DATABASE,
  READINESS_DATABASE_ID,
  READINESS_ENVIRONMENT,
  READINESS_WORKER,
} from "./admit-readiness-archive.mjs";
import { canonicalJson, sha256 } from "./readiness-artifact-contract.mjs";
import {
  SENDER_FILTER_ROUTE_CONTRACT,
} from "./sender-filter-deployment-contract.mjs";

export const DEPLOYMENT_EVIDENCE_ENVELOPE_FORMAT =
  "osl.keyserver.deployment-evidence-envelope.v2";
export const DEPLOYMENT_EVIDENCE_PAYLOAD_FORMAT =
  "osl.keyserver.deployment-evidence-payload.v2";
export const DEPLOYMENT_EVIDENCE_CHALLENGE_FORMAT =
  "osl.keyserver.deployment-evidence-challenge.v2";
export const DEPLOYMENT_EVIDENCE_DOMAIN =
  "OSL-KEYSERVER-DEPLOYMENT-EVIDENCE-v2\u0000";
export const DEPLOYMENT_EVIDENCE_MAX_ACTION_MS = 15 * 60_000;
export const DEPLOYMENT_EVIDENCE_MAX_AGE_MS = 120_000;
export const DEPLOYMENT_EVIDENCE_CLOCK_SKEW_MS = 10_000;
export const DEPLOYMENT_EVIDENCE_CHALLENGE_MAX_AGE_MS = 15 * 60_000;

// Enrollment requires a separately reviewed trust-root change. This registry
// intentionally stays empty, so source alone cannot trust or authorize anyone.
export const TRUSTED_DEPLOYMENT_EVIDENCE_PRODUCERS = Object.freeze({});

export const DEPLOYMENT_MIGRATION_LIST_QUERY = `SELECT
  id,
  name,
  applied_at
FROM d1_migrations
ORDER BY id ASC`;

export const DEPLOYMENT_SCHEMA_FINGERPRINT_QUERY = `SELECT
  type,
  name,
  tbl_name,
  sql
FROM sqlite_schema
WHERE name NOT LIKE 'sqlite_%'
ORDER BY type ASC, name ASC, tbl_name ASC, sql ASC`;

export const DEPLOYMENT_TRANSITIONS = Object.freeze({
  "legacy:A": "legacy-to-artifact-a",
  "legacy:B": "legacy-to-artifact-b",
  "A:A": "artifact-a-forward",
  "A:B": "artifact-a-to-artifact-b",
  "B:B": "artifact-b-forward",
});

const ZERO_SHA256 = "0".repeat(64);
const ENVELOPE_FIELDS = Object.freeze([
  "format",
  "payload",
  "payload_sha256",
  "producer_key_id",
  "signature_b64",
]);
const PAYLOAD_FIELDS = Object.freeze([
  "archive_id",
  "artifact",
  "artifact_bundle_sha256",
  "artifact_isolation",
  "challenge",
  "database",
  "expected_commit",
  "format",
  "migrations",
  "previous_receipt_sha256",
  "producer_identity",
  "producer_run_id",
  "producer_sequence",
  "sender_filter",
  "timestamps",
  "transition",
  "worker",
]);
const CHALLENGE_FIELDS = Object.freeze([
  "archive_id",
  "artifact",
  "artifact_bundle_sha256",
  "challenge_id",
  "expected_commit",
  "expires_at",
  "format",
  "issued_at",
  "nonce_b64",
  "permitted_transition",
  "previous_artifact",
  "previous_deployment_id",
  "previous_receipt_sha256",
  "previous_sequence",
  "previous_worker_version",
  "producer_identity",
  "producer_key_id",
]);

function requireObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

function requireExactKeys(value, expected, label) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((key, index) => key !== wanted[index])
  ) {
    throw new Error(`${label} fields are not exact`);
  }
}

function requireNonemptyString(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be nonempty`);
  }
}

function requirePositiveInteger(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${label} must be positive`);
  }
}

function requireSha256(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{64}$/.test(value) ||
    value === ZERO_SHA256
  ) {
    throw new Error(`${label} must be a nonzero lowercase SHA-256`);
  }
}

function requireGitCommit(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{40}$/.test(value) ||
    value === "0".repeat(40)
  ) {
    throw new Error(`${label} must be a nonzero full lowercase Git commit`);
  }
}

function requireUuid(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
      value,
    )
  ) {
    throw new Error(`${label} must be a lowercase UUID`);
  }
}

function decodeBase64(value, label, exactBytes) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be nonempty base64`);
  }
  const decoded = Buffer.from(value, "base64");
  if (
    decoded.toString("base64") !== value ||
    decoded.byteLength === 0 ||
    (exactBytes !== undefined && decoded.byteLength !== exactBytes)
  ) {
    throw new Error(`${label} is not canonical nonempty base64`);
  }
  return decoded;
}

function parseTimestamp(value, label) {
  if (
    typeof value !== "string" ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(value) ||
    !Number.isFinite(Date.parse(value))
  ) {
    throw new Error(`${label} must be a canonical ISO timestamp`);
  }
  return Date.parse(value);
}

function observationDigest(value) {
  return sha256(Buffer.from(canonicalJson(value)));
}

function requireRecomputedObservation(
  observation,
  claimedCount,
  claimedDigest,
  label,
) {
  const raw = requireObject(observation, `${label} raw observation`);
  requirePositiveInteger(claimedCount, `${label} raw cardinality`);
  if (Object.keys(raw).length !== claimedCount) {
    throw new Error(`${label} raw cardinality mismatch`);
  }
  requireSha256(claimedDigest, `${label} observation digest`);
  if (observationDigest(raw) !== claimedDigest) {
    throw new Error(`${label} observation digest mismatch`);
  }
  return raw;
}

function validateTimestamps(value, challenge, nowMs) {
  const timestamps = requireObject(value, "deployment evidence timestamps");
  const fields = [
    "action_started_at",
    "migrations_captured_at",
    "worker_deployed_at",
    "probes_finished_at",
    "issued_at",
  ];
  requireExactKeys(timestamps, fields, "deployment evidence timestamps");
  const values = fields.map((field) =>
    parseTimestamp(timestamps[field], `deployment evidence ${field}`),
  );
  for (let index = 1; index < values.length; index += 1) {
    if (values[index] < values[index - 1]) {
      throw new Error("deployment evidence timestamps are out of order");
    }
  }
  if (values.at(-1) - values[0] > DEPLOYMENT_EVIDENCE_MAX_ACTION_MS) {
    throw new Error("deployment evidence action interval is too long");
  }
  const issued = values.at(-1);
  if (
    issued < nowMs - DEPLOYMENT_EVIDENCE_MAX_AGE_MS ||
    issued > nowMs + DEPLOYMENT_EVIDENCE_CLOCK_SKEW_MS
  ) {
    throw new Error("deployment evidence is stale or future-dated");
  }
  const challengeIssued = parseTimestamp(
    challenge.issued_at,
    "verifier challenge issued_at",
  );
  const challengeExpires = parseTimestamp(
    challenge.expires_at,
    "verifier challenge expires_at",
  );
  if (
    challengeIssued > values[0] ||
    challengeExpires < issued ||
    challengeExpires <= challengeIssued ||
    challengeExpires - challengeIssued > DEPLOYMENT_EVIDENCE_CHALLENGE_MAX_AGE_MS
  ) {
    throw new Error("verifier challenge does not cover the action interval");
  }
}

function validateExpectedMigrations(value) {
  if (!Array.isArray(value) || value.length === 0) {
    throw new Error("expected committed migration closure is empty");
  }
  return value.map((entryValue, index) => {
    const entry = requireObject(entryValue, "expected committed migration");
    requireExactKeys(entry, ["name", "sha256"], "expected committed migration");
    if (
      typeof entry.name !== "string" ||
      !/^\d{4}_[a-z0-9_]+\.sql$/.test(entry.name)
    ) {
      throw new Error("expected committed migration name is invalid");
    }
    requireSha256(entry.sha256, "expected committed migration digest");
    if (index > 0 && value[index - 1].name >= entry.name) {
      throw new Error("expected committed migrations are not strictly ordered");
    }
    return entry;
  });
}

function validateMigrations(value, expectedMigrations, artifact) {
  if (!Array.isArray(value) || value.length === 0) {
    throw new Error("producer migration evidence is empty");
  }
  if (value.length > expectedMigrations.length) {
    throw new Error("producer migration list exceeds committed migrations");
  }
  value.forEach((entryValue, index) => {
    const entry = requireObject(entryValue, "producer migration entry");
    requireExactKeys(
      entry,
      ["applied_order", "name", "sha256"],
      "producer migration entry",
    );
    const expected = expectedMigrations[index];
    if (
      entry.applied_order !== index + 1 ||
      entry.name !== expected.name ||
      entry.sha256 !== expected.sha256
    ) {
      throw new Error("producer migration order or digest mismatch");
    }
  });
  if (artifact === "B") {
    if (
      value.length !== expectedMigrations.length ||
      !value.at(-1).name.startsWith("0031_") ||
      value.at(-2)?.name !==
        "0030_reserve_derived_identity_namespace.sql"
    ) {
      throw new Error("Artifact B requires the exact ordered 0030 then 0031 tail");
    }
  } else if (value.some((entry) => entry.name.startsWith("0031_"))) {
    throw new Error("Artifact A migration evidence includes migration 0031");
  }
  return value;
}

function validateSchemaRows(value) {
  if (!Array.isArray(value) || value.length === 0) {
    throw new Error("producer schema rows are empty");
  }
  let previous = null;
  for (const rowValue of value) {
    const row = requireObject(rowValue, "producer schema row");
    requireExactKeys(
      row,
      ["name", "sql", "tbl_name", "type"],
      "producer schema row",
    );
    requireNonemptyString(row.type, "producer schema type");
    requireNonemptyString(row.name, "producer schema name");
    requireNonemptyString(row.tbl_name, "producer schema table name");
    if (row.sql !== null) requireNonemptyString(row.sql, "producer schema SQL");
    const ordering = canonicalJson([
      row.type,
      row.name,
      row.tbl_name,
      row.sql,
    ]);
    if (previous !== null && previous >= ordering) {
      throw new Error("producer schema rows are not strictly ordered");
    }
    previous = ordering;
  }
  return value;
}

function validateMigrationRows(value, migrations) {
  if (!Array.isArray(value) || value.length === 0) {
    throw new Error("producer raw migration rows are empty");
  }
  if (value.length !== migrations.length) {
    throw new Error("producer raw migration row cardinality mismatch");
  }
  value.forEach((rowValue, index) => {
    const row = requireObject(rowValue, "producer raw migration row");
    requireExactKeys(
      row,
      ["applied_at", "id", "name"],
      "producer raw migration row",
    );
    requirePositiveInteger(row.id, "producer raw migration id");
    requireNonemptyString(row.applied_at, "producer migration applied_at");
    if (
      row.name !== migrations[index].name ||
      (index > 0 && row.id <= value[index - 1].id)
    ) {
      throw new Error("producer raw migration order mismatch");
    }
  });
  return value;
}

function validateDatabase(value, migrations) {
  const database = requireObject(value, "producer database evidence");
  requireExactKeys(
    database,
    [
      "environment",
      "id",
      "migration_list_output_sha256",
      "migration_list_query_sha256",
      "migration_row_count",
      "migration_rows",
      "name",
      "schema_fingerprint_sha256",
      "schema_object_count",
      "schema_output_sha256",
      "schema_query_sha256",
      "schema_rows",
    ],
    "producer database evidence",
  );
  if (
    database.name !== READINESS_DATABASE ||
    database.id !== READINESS_DATABASE_ID ||
    database.environment !== READINESS_ENVIRONMENT
  ) {
    throw new Error("producer database identity mismatch");
  }
  const migrationQueryDigest = sha256(
    Buffer.from(DEPLOYMENT_MIGRATION_LIST_QUERY),
  );
  const schemaQueryDigest = sha256(
    Buffer.from(DEPLOYMENT_SCHEMA_FINGERPRINT_QUERY),
  );
  requireSha256(
    database.migration_list_query_sha256,
    "producer migration query digest",
  );
  requireSha256(database.schema_query_sha256, "producer schema query digest");
  if (
    database.migration_list_query_sha256 !== migrationQueryDigest ||
    database.schema_query_sha256 !== schemaQueryDigest
  ) {
    throw new Error("producer database query contract mismatch");
  }
  requirePositiveInteger(
    database.migration_row_count,
    "producer migration row cardinality",
  );
  const migrationRows = validateMigrationRows(
    database.migration_rows,
    migrations,
  );
  if (database.migration_row_count !== migrationRows.length) {
    throw new Error("producer migration row cardinality mismatch");
  }
  requireSha256(
    database.migration_list_output_sha256,
    "producer migration output digest",
  );
  if (
    database.migration_list_output_sha256 !==
    observationDigest(migrationRows)
  ) {
    throw new Error("producer migration output digest mismatch");
  }
  const schemaRows = validateSchemaRows(database.schema_rows);
  requirePositiveInteger(
    database.schema_object_count,
    "producer schema object cardinality",
  );
  if (database.schema_object_count !== schemaRows.length) {
    throw new Error("producer schema object cardinality mismatch");
  }
  const schemaDigest = observationDigest(schemaRows);
  for (const field of [
    "schema_output_sha256",
    "schema_fingerprint_sha256",
  ]) {
    requireSha256(database[field], `producer database ${field}`);
    if (database[field] !== schemaDigest) {
      throw new Error("producer schema observation digest mismatch");
    }
  }
}

function validateChallenge(value, expectation, producerKeyId, producerIdentity) {
  const challenge = requireObject(value, "deployment evidence challenge");
  requireExactKeys(
    challenge,
    CHALLENGE_FIELDS,
    "deployment evidence challenge",
  );
  if (challenge.format !== DEPLOYMENT_EVIDENCE_CHALLENGE_FORMAT) {
    throw new Error("deployment evidence challenge format mismatch");
  }
  requireUuid(challenge.challenge_id, "verifier challenge id");
  decodeBase64(challenge.nonce_b64, "verifier challenge nonce", 32);
  requirePositiveInteger(
    challenge.previous_sequence,
    "verifier previous sequence",
  );
  requireSha256(
    challenge.previous_receipt_sha256,
    "verifier previous receipt digest",
  );
  requireUuid(
    challenge.previous_worker_version,
    "verifier previous Worker version",
  );
  requireUuid(
    challenge.previous_deployment_id,
    "verifier previous deployment id",
  );
  if (!["legacy", "A", "B"].includes(challenge.previous_artifact)) {
    throw new Error("verifier previous artifact is invalid");
  }
  const transition =
    DEPLOYMENT_TRANSITIONS[
      `${challenge.previous_artifact}:${challenge.artifact}`
    ];
  if (!transition || transition !== challenge.permitted_transition) {
    throw new Error("verifier challenge transition is not permitted");
  }
  requireGitCommit(challenge.expected_commit, "challenge expected commit");
  requireSha256(challenge.archive_id, "challenge archive");
  requireSha256(challenge.artifact_bundle_sha256, "challenge artifact bundle");
  if (
    challenge.producer_key_id !== producerKeyId ||
    challenge.producer_identity !== producerIdentity ||
    challenge.expected_commit !== expectation.expectedCommit ||
    challenge.archive_id !== expectation.archiveId ||
    challenge.artifact !== expectation.artifact ||
    challenge.artifact_bundle_sha256 !==
      expectation.artifactBundles[expectation.artifact] ||
    canonicalJson(challenge) !== canonicalJson(expectation.verifierChallenge)
  ) {
    throw new Error("receipt does not carry the verifier-issued challenge");
  }
  return challenge;
}

function validateTransition(value, challenge, expectation) {
  const transition = requireObject(value, "deployment transition");
  requireExactKeys(
    transition,
    [
      "current_artifact",
      "current_deployment_id",
      "current_worker_version",
      "permitted_transition",
      "previous_artifact",
      "previous_deployment_id",
      "previous_worker_version",
    ],
    "deployment transition",
  );
  if (
    transition.previous_artifact !== challenge.previous_artifact ||
    transition.previous_worker_version !== challenge.previous_worker_version ||
    transition.previous_deployment_id !== challenge.previous_deployment_id ||
    transition.current_artifact !== expectation.artifact ||
    transition.current_worker_version !== expectation.expectedWorkerVersion ||
    transition.current_deployment_id !== expectation.expectedDeploymentId ||
    transition.permitted_transition !== challenge.permitted_transition
  ) {
    throw new Error("deployment transition does not match verifier state");
  }
  requireUuid(transition.current_worker_version, "current Worker version");
  requireUuid(transition.current_deployment_id, "current deployment id");
  if (
    transition.current_worker_version === transition.previous_worker_version ||
    transition.current_deployment_id === transition.previous_deployment_id
  ) {
    throw new Error("deployment transition did not advance Worker identity");
  }
}

function validateHealthRoute(value) {
  const health = requireObject(value, "producer health route");
  requireExactKeys(
    health,
    [
      "method",
      "path",
      "response",
      "response_field_count",
      "response_sha256",
      "status",
    ],
    "producer health route",
  );
  if (
    health.method !== "GET" ||
    health.path !== SENDER_FILTER_ROUTE_CONTRACT.health_path ||
    health.status !== 200
  ) {
    throw new Error("producer health route contract mismatch");
  }
  return requireRecomputedObservation(
    health.response,
    health.response_field_count,
    health.response_sha256,
    "producer health response",
  );
}

function validateSenderRoute(value, artifact) {
  const sender = requireObject(value, "producer sender-filter route");
  requireExactKeys(
    sender,
    [
      "filtered_sender_id",
      "item_count",
      "method",
      "path",
      "request_signature_b64",
      "request_signature_byte_count",
      "request_signature_sha256",
      "response",
      "response_field_count",
      "response_sha256",
      "status",
    ],
    "producer sender-filter route",
  );
  if (
    sender.method !== SENDER_FILTER_ROUTE_CONTRACT.method ||
    sender.path !== SENDER_FILTER_ROUTE_CONTRACT.path
  ) {
    throw new Error("producer sender-filter route contract mismatch");
  }
  const signature = decodeBase64(
    sender.request_signature_b64,
    "producer sender-filter request signature",
  );
  requirePositiveInteger(
    sender.request_signature_byte_count,
    "producer request signature cardinality",
  );
  requireSha256(
    sender.request_signature_sha256,
    "producer signed sender request digest",
  );
  if (
    sender.request_signature_byte_count !== signature.byteLength ||
    sender.request_signature_sha256 !== sha256(signature)
  ) {
    throw new Error("producer signed sender request observation mismatch");
  }
  const response = requireRecomputedObservation(
    sender.response,
    sender.response_field_count,
    sender.response_sha256,
    "producer sender-filter response",
  );
  if (artifact === "B") {
    if (
      sender.status !== 200 ||
      typeof sender.filtered_sender_id !== "string" ||
      sender.filtered_sender_id.length === 0 ||
      !Number.isSafeInteger(sender.item_count) ||
      sender.item_count <= 0 ||
      response.filtered_sender_id !== sender.filtered_sender_id ||
      !Array.isArray(response.items) ||
      response.items.length !== sender.item_count ||
      response.items.some(
        (item) =>
          !item ||
          item.sender_id !== sender.filtered_sender_id ||
          typeof item.bundle_b64 !== "string" ||
          decodeBase64(item.bundle_b64, "producer sender item bundle")
            .byteLength === 0,
      )
    ) {
      throw new Error("Artifact B sender-filter route proof is empty or mismatched");
    }
  } else if (
    sender.status !== 503 ||
    sender.filtered_sender_id !== null ||
    sender.item_count !== 0 ||
    typeof response.error !== "string" ||
    response.error.length === 0
  ) {
    throw new Error("Artifact A did not preserve sender-filter refusal");
  }
  return { sender, response };
}

function validateWorker(value, expectation) {
  const worker = requireObject(value, "producer Worker evidence");
  requireExactKeys(
    worker,
    [
      "bundle_sha256",
      "deployment_id",
      "deployment_observation",
      "deployment_observation_field_count",
      "deployment_observation_sha256",
      "health_route",
      "sender_filter_route",
      "service",
      "version_id",
    ],
    "producer Worker evidence",
  );
  requireUuid(worker.version_id, "producer Worker version");
  requireUuid(worker.deployment_id, "producer deployment id");
  requireSha256(worker.bundle_sha256, "producer Worker bundle digest");
  if (
    worker.service !== READINESS_WORKER ||
    worker.version_id !== expectation.expectedWorkerVersion ||
    worker.deployment_id !== expectation.expectedDeploymentId ||
    worker.bundle_sha256 !== expectation.artifactBundles[expectation.artifact]
  ) {
    throw new Error("producer Worker identity or bundle mismatch");
  }
  const deployment = requireRecomputedObservation(
    worker.deployment_observation,
    worker.deployment_observation_field_count,
    worker.deployment_observation_sha256,
    "producer Worker deployment",
  );
  requireExactKeys(
    deployment,
    ["bundle_sha256", "deployment_id", "service", "version_id"],
    "producer Worker deployment observation",
  );
  if (
    deployment.service !== worker.service ||
    deployment.version_id !== worker.version_id ||
    deployment.deployment_id !== worker.deployment_id ||
    deployment.bundle_sha256 !== worker.bundle_sha256
  ) {
    throw new Error("producer Worker deployment observation mismatch");
  }
  const healthResponse = validateHealthRoute(worker.health_route);
  const sender = validateSenderRoute(
    worker.sender_filter_route,
    expectation.artifact,
  );
  return { worker, healthResponse, sender };
}

function validateSenderFilter(value, workerEvidence, artifact) {
  const capability = requireObject(
    value,
    "producer sender-filter capability",
  );
  requireExactKeys(
    capability,
    [
      "advertised",
      "health_response_sha256",
      "name",
      "probe_sender_id",
      "version",
    ],
    "producer sender-filter capability",
  );
  requireSha256(
    capability.health_response_sha256,
    "producer capability health digest",
  );
  if (
    capability.name !== SENDER_FILTER_ROUTE_CONTRACT.health_capability ||
    capability.health_response_sha256 !==
      workerEvidence.worker.health_route.response_sha256
  ) {
    throw new Error("producer sender-filter capability identity mismatch");
  }
  const advertisedVersion =
    workerEvidence.healthResponse.capabilities?.[
      SENDER_FILTER_ROUTE_CONTRACT.health_capability
    ];
  if (artifact === "B") {
    if (
      capability.advertised !== true ||
      capability.version !==
        SENDER_FILTER_ROUTE_CONTRACT.health_capability_version ||
      advertisedVersion !== capability.version ||
      capability.probe_sender_id !==
        workerEvidence.worker.sender_filter_route.filtered_sender_id
    ) {
      throw new Error("Artifact B capability advertisement mismatch");
    }
  } else if (
    capability.advertised !== false ||
    capability.version !== null ||
    capability.probe_sender_id !== null ||
    advertisedVersion !== undefined
  ) {
    throw new Error("Artifact A falsely advertises sender-filter capability");
  }
}

function validateArtifactProbe(value, label, expectedBundle) {
  const probe = requireObject(value, `producer ${label} isolation probe`);
  requireExactKeys(
    probe,
    [
      "active",
      "bundle_sha256",
      "observation",
      "observation_field_count",
      "observed_version_id",
      "probe_sha256",
    ],
    `producer ${label} isolation probe`,
  );
  requireSha256(probe.bundle_sha256, `producer ${label} bundle digest`);
  if (probe.bundle_sha256 !== expectedBundle) {
    throw new Error(`producer ${label} bundle mismatch`);
  }
  const observation = requireRecomputedObservation(
    probe.observation,
    probe.observation_field_count,
    probe.probe_sha256,
    `producer ${label} probe`,
  );
  requireExactKeys(
    observation,
    ["active", "bundle_sha256", "observed_version_id"],
    `producer ${label} probe observation`,
  );
  if (
    observation.active !== probe.active ||
    observation.bundle_sha256 !== probe.bundle_sha256 ||
    observation.observed_version_id !== probe.observed_version_id
  ) {
    throw new Error(`producer ${label} probe observation mismatch`);
  }
  if (probe.active === true) {
    requireUuid(probe.observed_version_id, `producer ${label} observed version`);
  } else if (
    probe.active !== false ||
    probe.observed_version_id !== null
  ) {
    throw new Error(`inactive ${label} probe has a Worker version`);
  }
  return probe;
}

function validateArtifactIsolation(value, expectation) {
  const isolation = requireObject(
    value,
    "producer artifact isolation evidence",
  );
  requireExactKeys(
    isolation,
    [
      "artifact_a",
      "artifact_b",
      "probe_nonce_b64",
      "probe_nonce_byte_count",
      "probe_nonce_sha256",
    ],
    "producer artifact isolation evidence",
  );
  const nonce = decodeBase64(
    isolation.probe_nonce_b64,
    "producer artifact isolation nonce",
  );
  requirePositiveInteger(
    isolation.probe_nonce_byte_count,
    "producer isolation nonce cardinality",
  );
  requireSha256(
    isolation.probe_nonce_sha256,
    "producer artifact isolation nonce digest",
  );
  if (
    isolation.probe_nonce_byte_count !== nonce.byteLength ||
    isolation.probe_nonce_sha256 !== sha256(nonce)
  ) {
    throw new Error("producer artifact isolation nonce observation mismatch");
  }
  const artifactA = validateArtifactProbe(
    isolation.artifact_a,
    "Artifact A",
    expectation.artifactBundles.A,
  );
  const artifactB = validateArtifactProbe(
    isolation.artifact_b,
    "Artifact B",
    expectation.artifactBundles.B,
  );
  const selected = expectation.artifact === "A" ? artifactA : artifactB;
  const unselected = expectation.artifact === "A" ? artifactB : artifactA;
  if (
    selected.active !== true ||
    selected.observed_version_id !== expectation.expectedWorkerVersion ||
    unselected.active !== false ||
    artifactA.bundle_sha256 === artifactB.bundle_sha256
  ) {
    throw new Error("producer Artifact A/B isolation mismatch");
  }
}

export function deploymentEvidenceSigningBytes(payload) {
  return Buffer.concat([
    Buffer.from(DEPLOYMENT_EVIDENCE_DOMAIN),
    Buffer.from(canonicalJson(payload)),
  ]);
}

export function validateDeploymentEvidenceExpectation(value) {
  const expectation = requireObject(
    value,
    "deployment evidence expectation",
  );
  requireExactKeys(
    expectation,
    [
      "archiveId",
      "artifact",
      "artifactBundles",
      "expectedCommit",
      "expectedDeploymentId",
      "expectedMigrations",
      "expectedWorkerVersion",
      "verifierChallenge",
    ],
    "deployment evidence expectation",
  );
  requireGitCommit(expectation.expectedCommit, "expected deployment commit");
  requireSha256(expectation.archiveId, "expected deployment archive");
  if (expectation.artifact !== "A" && expectation.artifact !== "B") {
    throw new Error("expected deployment artifact must be A or B");
  }
  requireUuid(expectation.expectedWorkerVersion, "expected Worker version");
  requireUuid(expectation.expectedDeploymentId, "expected deployment id");
  const bundles = requireObject(
    expectation.artifactBundles,
    "expected artifact bundles",
  );
  requireExactKeys(bundles, ["A", "B"], "expected artifact bundles");
  requireSha256(bundles.A, "expected Artifact A bundle");
  requireSha256(bundles.B, "expected Artifact B bundle");
  if (bundles.A === bundles.B) {
    throw new Error("expected Artifact A and B bundles are identical");
  }
  requireObject(expectation.verifierChallenge, "expected verifier challenge");
  return {
    ...expectation,
    expectedMigrations: validateExpectedMigrations(
      expectation.expectedMigrations,
    ),
  };
}

export function verifyDeploymentEvidenceReceipt(
  envelopeValue,
  expectationValue,
  {
    trustedProducers = TRUSTED_DEPLOYMENT_EVIDENCE_PRODUCERS,
    nowMs = Date.now(),
  } = {},
) {
  const expectation = validateDeploymentEvidenceExpectation(expectationValue);
  const envelope = requireObject(
    envelopeValue,
    "deployment evidence envelope",
  );
  requireExactKeys(
    envelope,
    ENVELOPE_FIELDS,
    "deployment evidence envelope",
  );
  if (envelope.format !== DEPLOYMENT_EVIDENCE_ENVELOPE_FORMAT) {
    throw new Error("deployment evidence envelope format mismatch");
  }
  requireNonemptyString(envelope.producer_key_id, "producer key id");
  const producer = trustedProducers[envelope.producer_key_id];
  if (!producer) {
    throw new Error("deployment evidence producer is not independently trusted");
  }
  requireExactKeys(
    producer,
    ["identity", "public_key_spki_b64"],
    "trusted deployment evidence producer",
  );
  requireNonemptyString(producer.identity, "trusted producer identity");
  const publicKey = createPublicKey({
    key: decodeBase64(
      producer.public_key_spki_b64,
      "trusted producer public key",
    ),
    format: "der",
    type: "spki",
  });

  const payload = requireObject(envelope.payload, "deployment evidence payload");
  requireExactKeys(payload, PAYLOAD_FIELDS, "deployment evidence payload");
  if (payload.format !== DEPLOYMENT_EVIDENCE_PAYLOAD_FORMAT) {
    throw new Error("deployment evidence payload format mismatch");
  }
  const payloadBytes = Buffer.from(canonicalJson(payload));
  requireSha256(envelope.payload_sha256, "deployment evidence payload digest");
  if (sha256(payloadBytes) !== envelope.payload_sha256) {
    throw new Error("deployment evidence payload digest mismatch");
  }
  const signature = decodeBase64(
    envelope.signature_b64,
    "deployment evidence signature",
    64,
  );
  if (
    !verifySignature(
      null,
      deploymentEvidenceSigningBytes(payload),
      publicKey,
      signature,
    )
  ) {
    throw new Error("deployment evidence producer signature is invalid");
  }
  if (payload.producer_identity !== producer.identity) {
    throw new Error("deployment evidence producer identity mismatch");
  }
  requireUuid(payload.producer_run_id, "producer run id");
  requirePositiveInteger(payload.producer_sequence, "producer sequence");
  requireSha256(
    payload.previous_receipt_sha256,
    "previous producer receipt digest",
  );
  requireGitCommit(payload.expected_commit, "producer expected commit");
  requireSha256(payload.archive_id, "producer archive id");
  requireSha256(payload.artifact_bundle_sha256, "producer artifact bundle");
  if (
    payload.expected_commit !== expectation.expectedCommit ||
    payload.archive_id !== expectation.archiveId ||
    payload.artifact !== expectation.artifact ||
    payload.artifact_bundle_sha256 !==
      expectation.artifactBundles[expectation.artifact]
  ) {
    throw new Error("producer commit, archive, artifact, or bundle mismatch");
  }
  const challenge = validateChallenge(
    payload.challenge,
    expectation,
    envelope.producer_key_id,
    producer.identity,
  );
  if (
    payload.producer_sequence !== challenge.previous_sequence + 1 ||
    payload.previous_receipt_sha256 !== challenge.previous_receipt_sha256
  ) {
    throw new Error("producer receipt does not advance verifier state");
  }
  validateTransition(payload.transition, challenge, expectation);
  validateTimestamps(payload.timestamps, challenge, nowMs);
  const migrations = validateMigrations(
    payload.migrations,
    expectation.expectedMigrations,
    expectation.artifact,
  );
  validateDatabase(payload.database, migrations);
  const worker = validateWorker(payload.worker, expectation);
  validateSenderFilter(payload.sender_filter, worker, expectation.artifact);
  validateArtifactIsolation(payload.artifact_isolation, expectation);

  return {
    envelope,
    payload,
    producer_key_id: envelope.producer_key_id,
    producer_identity: producer.identity,
    receipt_sha256: sha256(Buffer.from(canonicalJson(envelope))),
  };
}
