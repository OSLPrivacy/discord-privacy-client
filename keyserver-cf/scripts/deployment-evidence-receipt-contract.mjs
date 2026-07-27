import {
  createPublicKey,
  verify as verifySignature,
} from "node:crypto";
import {
  READINESS_DATABASE,
  READINESS_DATABASE_ID,
  READINESS_ENVIRONMENT,
  READINESS_WORKER,
} from "./admit-readiness-archive.mjs";
import {
  canonicalJson,
  sha256,
} from "./readiness-artifact-contract.mjs";
import {
  SENDER_FILTER_ROUTE_CONTRACT,
} from "./sender-filter-deployment-contract.mjs";

export const DEPLOYMENT_EVIDENCE_ENVELOPE_FORMAT =
  "osl.keyserver.deployment-evidence-envelope.v1";
export const DEPLOYMENT_EVIDENCE_PAYLOAD_FORMAT =
  "osl.keyserver.deployment-evidence-payload.v1";
export const DEPLOYMENT_EVIDENCE_DOMAIN =
  "OSL-KEYSERVER-DEPLOYMENT-EVIDENCE-v1\u0000";
export const DEPLOYMENT_EVIDENCE_MAX_ACTION_MS = 15 * 60_000;
export const DEPLOYMENT_EVIDENCE_MAX_AGE_MS = 120_000;
export const DEPLOYMENT_EVIDENCE_CLOCK_SKEW_MS = 10_000;

// A production key must be enrolled by a separate, reviewed trust-root change.
// An empty registry is deliberate: it makes every real receipt fail closed.
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
  "authorization_id",
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
  "worker",
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

function requireSha256(value, label) {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value)) {
    throw new Error(`${label} must be a lowercase SHA-256`);
  }
}

function requireGitCommit(value, label) {
  if (typeof value !== "string" || !/^[0-9a-f]{40}$/.test(value)) {
    throw new Error(`${label} must be a full lowercase Git commit`);
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
    (exactBytes !== undefined && decoded.byteLength !== exactBytes)
  ) {
    throw new Error(`${label} is not canonical base64`);
  }
  return decoded;
}

function requireTimestamp(value, label) {
  if (
    typeof value !== "string" ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(value) ||
    !Number.isFinite(Date.parse(value))
  ) {
    throw new Error(`${label} must be a canonical ISO timestamp`);
  }
  return Date.parse(value);
}

function validateTimestamps(value, nowMs) {
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
    requireTimestamp(timestamps[field], `deployment evidence ${field}`),
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
  return timestamps;
}

function validateExpectedMigrations(value) {
  if (!Array.isArray(value) || value.length === 0) {
    throw new Error("expected committed migration closure is empty");
  }
  return value.map((entryValue, index) => {
    const entry = requireObject(entryValue, "expected committed migration");
    requireExactKeys(
      entry,
      ["name", "sha256"],
      "expected committed migration",
    );
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
  const lastName = value.at(-1).name;
  if (artifact === "B") {
    if (
      value.length !== expectedMigrations.length ||
      !lastName.startsWith("0031_") ||
      value.at(-2)?.name !==
        "0030_reserve_derived_identity_namespace.sql"
    ) {
      throw new Error("Artifact B requires the exact ordered 0030 then 0031 tail");
    }
  } else if (
    value.some((entry) => entry.name.startsWith("0031_"))
  ) {
    throw new Error("Artifact A migration evidence includes migration 0031");
  }
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
      "name",
      "schema_fingerprint_sha256",
      "schema_object_count",
      "schema_output_sha256",
      "schema_query_sha256",
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
  if (
    database.migration_list_query_sha256 !==
      sha256(Buffer.from(DEPLOYMENT_MIGRATION_LIST_QUERY)) ||
    database.schema_query_sha256 !==
      sha256(Buffer.from(DEPLOYMENT_SCHEMA_FINGERPRINT_QUERY))
  ) {
    throw new Error("producer database query contract mismatch");
  }
  for (const field of [
    "migration_list_output_sha256",
    "schema_fingerprint_sha256",
    "schema_output_sha256",
  ]) {
    requireSha256(database[field], `producer database ${field}`);
    if (database[field] === "0".repeat(64)) {
      throw new Error(`producer database ${field} is an empty sentinel`);
    }
  }
  if (
    database.migration_list_output_sha256 !==
      sha256(Buffer.from(canonicalJson(migrations))) ||
    database.schema_output_sha256 !==
      database.schema_fingerprint_sha256
  ) {
    throw new Error("producer database output or schema fingerprint mismatch");
  }
  if (
    !Number.isSafeInteger(database.schema_object_count) ||
    database.schema_object_count <= 0 ||
    migrations.length === 0
  ) {
    throw new Error("producer schema evidence is empty");
  }
  return database;
}

function validateWorker(value, expectation) {
  const worker = requireObject(value, "producer Worker evidence");
  requireExactKeys(
    worker,
    [
      "bundle_sha256",
      "deployment_id",
      "health_route",
      "sender_filter_route",
      "service",
      "version_id",
    ],
    "producer Worker evidence",
  );
  if (
    worker.service !== READINESS_WORKER ||
    worker.version_id !== expectation.expectedWorkerVersion ||
    worker.deployment_id !== expectation.expectedDeploymentId ||
    worker.bundle_sha256 !== expectation.artifactBundles[expectation.artifact]
  ) {
    throw new Error("producer Worker identity or bundle mismatch");
  }
  requireUuid(worker.version_id, "producer Worker version");
  requireUuid(worker.deployment_id, "producer deployment id");
  requireSha256(worker.bundle_sha256, "producer Worker bundle digest");

  const health = requireObject(worker.health_route, "producer health route");
  requireExactKeys(
    health,
    ["method", "path", "response_sha256", "status"],
    "producer health route",
  );
  if (
    health.method !== "GET" ||
    health.path !== SENDER_FILTER_ROUTE_CONTRACT.health_path ||
    health.status !== 200
  ) {
    throw new Error("producer health route contract mismatch");
  }
  requireSha256(health.response_sha256, "producer health response digest");

  const sender = requireObject(
    worker.sender_filter_route,
    "producer sender-filter route",
  );
  requireExactKeys(
    sender,
    [
      "filtered_sender_id",
      "item_count",
      "method",
      "path",
      "request_signature_sha256",
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
  requireSha256(
    sender.request_signature_sha256,
    "producer signed sender request digest",
  );
  requireSha256(sender.response_sha256, "producer sender response digest");
  if (expectation.artifact === "B") {
    if (
      sender.status !== 200 ||
      typeof sender.filtered_sender_id !== "string" ||
      sender.filtered_sender_id.length === 0 ||
      !Number.isSafeInteger(sender.item_count) ||
      sender.item_count <= 0
    ) {
      throw new Error("Artifact B sender-filter route proof is empty or mismatched");
    }
  } else if (
    sender.status !== 503 ||
    sender.filtered_sender_id !== null ||
    sender.item_count !== 0
  ) {
    throw new Error("Artifact A did not preserve sender-filter refusal");
  }
  return worker;
}

function validateSenderFilter(value, worker, artifact) {
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
  if (
    capability.name !== SENDER_FILTER_ROUTE_CONTRACT.health_capability ||
    capability.health_response_sha256 !== worker.health_route.response_sha256
  ) {
    throw new Error("producer sender-filter capability identity mismatch");
  }
  if (artifact === "B") {
    if (
      capability.advertised !== true ||
      capability.version !==
        SENDER_FILTER_ROUTE_CONTRACT.health_capability_version ||
      capability.probe_sender_id !==
        worker.sender_filter_route.filtered_sender_id
    ) {
      throw new Error("Artifact B capability advertisement mismatch");
    }
  } else if (
    capability.advertised !== false ||
    capability.version !== null ||
    capability.probe_sender_id !== null
  ) {
    throw new Error("Artifact A falsely advertises sender-filter capability");
  }
  return capability;
}

function validateArtifactProbe(value, label, expectedBundle) {
  const probe = requireObject(value, `producer ${label} isolation probe`);
  requireExactKeys(
    probe,
    [
      "active",
      "bundle_sha256",
      "observed_version_id",
      "probe_sha256",
    ],
    `producer ${label} isolation probe`,
  );
  if (probe.bundle_sha256 !== expectedBundle) {
    throw new Error(`producer ${label} bundle mismatch`);
  }
  requireSha256(probe.bundle_sha256, `producer ${label} bundle digest`);
  requireSha256(probe.probe_sha256, `producer ${label} probe digest`);
  if (probe.active) {
    requireUuid(probe.observed_version_id, `producer ${label} observed version`);
  } else if (probe.observed_version_id !== null) {
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
    ["artifact_a", "artifact_b", "probe_nonce_sha256"],
    "producer artifact isolation evidence",
  );
  requireSha256(
    isolation.probe_nonce_sha256,
    "producer artifact isolation nonce",
  );
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
    unselected.active !== false
  ) {
    throw new Error("producer Artifact A/B isolation mismatch");
  }
  if (artifactA.bundle_sha256 === artifactB.bundle_sha256) {
    throw new Error("Artifact A and B bundles are not isolated");
  }
  return isolation;
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
  requireUuid(payload.authorization_id, "producer authorization id");
  if (
    !Number.isSafeInteger(payload.producer_sequence) ||
    payload.producer_sequence <= 0
  ) {
    throw new Error("producer sequence must be positive");
  }
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

  validateTimestamps(payload.timestamps, nowMs);
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
