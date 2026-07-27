import {
  ADMISSION_FORMAT,
  READINESS_DATABASE,
  READINESS_DATABASE_CLOCK_SKEW_MS,
  READINESS_DATABASE_ID,
  READINESS_ENVIRONMENT,
  READINESS_MARKERS_QUERY,
  READINESS_SCHEMA_QUERY,
} from "./admit-readiness-archive.mjs";
import {
  validateReadinessWorkerPlan,
} from "./readiness-artifact-contract.mjs";
import {
  SENDER_FILTER_RECEIPT_FORMAT,
  SENDER_FILTER_ROUTE_CONTRACT,
  SENDER_FILTER_SOURCE_FILES,
  sha256,
} from "./sender-filter-deployment-contract.mjs";
import {
  verifyDeploymentEvidenceReceipt,
} from "./deployment-evidence-receipt-contract.mjs";

export const TRUSTED_RELEASE_SELECTION_FORMAT =
  "osl.keyserver.trusted-release-selection.v1";
export const TRUSTED_RELEASE_MAX_AGE_MS = 120_000;
export const TRUSTED_RELEASE_CLOCK_SKEW_MS = 10_000;

const READINESS_FIELDS = Object.freeze([
  "active_worker_version",
  "admitted",
  "archive_id",
  "artifact",
  "artifact_bundles",
  "capability_table_exists",
  "captured_finished_at",
  "captured_started_at",
  "database",
  "database_id",
  "database_unix_time",
  "deployment_id",
  "deployment_status_after_sha256",
  "deployment_status_before_sha256",
  "environment",
  "expected_commit",
  "format",
  "markers",
  "markers_query_sha256",
  "schema_query_sha256",
]);

const SOURCE_RECEIPT_FIELDS = Object.freeze([
  "admission_scope",
  "deployment_admitted",
  "format",
  "generated_at",
  "live_evidence",
  "payload_sha256",
  "refusal_reasons",
  "route_contract",
  "source",
  "source_contract_admitted",
  "would_admit_with_trusted_live_evidence",
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

function requireGitObject(value, label) {
  if (typeof value !== "string" || !/^[0-9a-f]{40}$/.test(value)) {
    throw new Error(`${label} must be a full lowercase Git object id`);
  }
}

function requireSha256(value, label) {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value)) {
    throw new Error(`${label} must be a lowercase SHA-256`);
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

function requireFreshInterval(startedAt, finishedAt, nowMs, label) {
  const started = Date.parse(startedAt);
  const finished = Date.parse(finishedAt);
  if (!Number.isFinite(started) || !Number.isFinite(finished)) {
    throw new Error(`${label} timestamps must be ISO timestamps`);
  }
  if (finished < started || finished - started > TRUSTED_RELEASE_MAX_AGE_MS) {
    throw new Error(`${label} capture interval is invalid`);
  }
  if (
    finished < nowMs - TRUSTED_RELEASE_MAX_AGE_MS ||
    finished > nowMs + TRUSTED_RELEASE_CLOCK_SKEW_MS
  ) {
    throw new Error(`${label} evidence is stale or future-dated`);
  }
}

function validateSourceReceipt(receiptValue, expectedCommit, nowMs) {
  const receipt = requireObject(
    receiptValue,
    "sender-filter source receipt",
  );
  requireExactKeys(
    receipt,
    SOURCE_RECEIPT_FIELDS,
    "sender-filter source receipt",
  );
  if (
    receipt.format !== SENDER_FILTER_RECEIPT_FORMAT ||
    receipt.admission_scope !== "source-only-non-authorizing" ||
    receipt.source_contract_admitted !== true ||
    receipt.deployment_admitted !== false
  ) {
    throw new Error("sender-filter receipt is not the source-only contract");
  }
  requireFreshInterval(
    receipt.generated_at,
    receipt.generated_at,
    nowMs,
    "sender-filter source receipt",
  );

  const payload = { ...receipt };
  delete payload.payload_sha256;
  requireSha256(receipt.payload_sha256, "sender-filter payload digest");
  if (
    sha256(Buffer.from(JSON.stringify(payload))) !== receipt.payload_sha256
  ) {
    throw new Error("sender-filter receipt payload digest mismatch");
  }

  const source = requireObject(receipt.source, "sender-filter source");
  requireExactKeys(
    source,
    ["commit", "files", "keyserver_tree", "repository_tree"],
    "sender-filter source",
  );
  requireGitObject(source.commit, "sender-filter source commit");
  requireGitObject(source.repository_tree, "sender-filter repository tree");
  requireGitObject(source.keyserver_tree, "sender-filter keyserver tree");
  if (source.commit !== expectedCommit) {
    throw new Error("sender-filter source commit mismatch");
  }

  const expectedPaths = Object.keys(SENDER_FILTER_SOURCE_FILES).sort();
  if (!Array.isArray(source.files) || source.files.length === 0) {
    throw new Error("sender-filter source fixture is empty");
  }
  const actualPaths = source.files.map((entry) => entry.path).sort();
  if (
    actualPaths.length !== expectedPaths.length ||
    actualPaths.some((entry, index) => entry !== expectedPaths[index])
  ) {
    throw new Error("sender-filter source file set mismatch");
  }
  for (const entryValue of source.files) {
    const entry = requireObject(entryValue, "sender-filter source file");
    requireExactKeys(
      entry,
      ["bytes", "path", "role", "sha256"],
      "sender-filter source file",
    );
    const expected = SENDER_FILTER_SOURCE_FILES[entry.path];
    if (
      !expected ||
      entry.role !== expected.role ||
      entry.sha256 !== expected.sha256 ||
      !Number.isSafeInteger(entry.bytes) ||
      entry.bytes <= 0
    ) {
      throw new Error(`sender-filter source file mismatch: ${entry.path}`);
    }
  }
  if (
    JSON.stringify(receipt.route_contract) !==
    JSON.stringify(SENDER_FILTER_ROUTE_CONTRACT)
  ) {
    throw new Error("sender-filter route contract mismatch");
  }
  return source;
}

function validateReadinessReceipt(
  receiptValue,
  { expectedCommit, artifact, expectedActiveVersion, nowMs },
) {
  const receipt = requireObject(receiptValue, "trusted readiness receipt");
  requireExactKeys(receipt, READINESS_FIELDS, "trusted readiness receipt");
  if (receipt.format !== ADMISSION_FORMAT || receipt.admitted !== true) {
    throw new Error("trusted readiness receipt is not admitted");
  }
  if (
    receipt.expected_commit !== expectedCommit ||
    receipt.artifact !== artifact
  ) {
    throw new Error("trusted readiness commit or artifact mismatch");
  }
  requireGitObject(receipt.expected_commit, "trusted readiness commit");
  requireSha256(receipt.archive_id, "trusted readiness archive id");
  const artifactBundles = requireObject(
    receipt.artifact_bundles,
    "trusted readiness artifact bundles",
  );
  requireExactKeys(
    artifactBundles,
    ["A", "B"],
    "trusted readiness artifact bundles",
  );
  requireSha256(artifactBundles.A, "trusted readiness Artifact A bundle");
  requireSha256(artifactBundles.B, "trusted readiness Artifact B bundle");
  if (artifactBundles.A === artifactBundles.B) {
    throw new Error("trusted readiness Artifact A and B bundles are identical");
  }
  requireUuid(receipt.active_worker_version, "active Worker version");
  requireUuid(receipt.deployment_id, "active deployment id");
  if (receipt.active_worker_version !== expectedActiveVersion) {
    throw new Error("active Worker version mismatch");
  }
  if (
    receipt.database !== READINESS_DATABASE ||
    receipt.database_id !== READINESS_DATABASE_ID ||
    receipt.environment !== READINESS_ENVIRONMENT
  ) {
    throw new Error("trusted readiness database or environment mismatch");
  }
  for (const field of [
    "deployment_status_before_sha256",
    "deployment_status_after_sha256",
  ]) {
    requireSha256(receipt[field], `trusted readiness ${field}`);
  }
  if (
    receipt.schema_query_sha256 !==
      sha256(Buffer.from(READINESS_SCHEMA_QUERY)) ||
    receipt.markers_query_sha256 !==
      sha256(Buffer.from(READINESS_MARKERS_QUERY))
  ) {
    throw new Error("trusted readiness query contract mismatch");
  }
  requireFreshInterval(
    receipt.captured_started_at,
    receipt.captured_finished_at,
    nowMs,
    "trusted readiness",
  );
  if (
    !Number.isSafeInteger(receipt.database_unix_time) ||
    receipt.database_unix_time <= 0
  ) {
    throw new Error("trusted readiness database time is invalid");
  }
  if (
    Math.abs(
      receipt.database_unix_time * 1000 -
        Date.parse(receipt.captured_finished_at),
    ) > READINESS_DATABASE_CLOCK_SKEW_MS
  ) {
    throw new Error("trusted readiness database time is stale or mismatched");
  }

  const markers = requireObject(receipt.markers, "trusted readiness markers");
  requireExactKeys(
    markers,
    [
      "control_inbox_sender_disposition",
      "control_inbox_sender_reconciliation_started",
    ],
    "trusted readiness markers",
  );
  validateReadinessWorkerPlan(
    artifact === "A" ? "artifact-a-bridge" : "artifact-b-final",
    {
      capability_table_exists: receipt.capability_table_exists,
      control_inbox_sender_disposition:
        markers.control_inbox_sender_disposition,
      control_inbox_sender_reconciliation_started:
        markers.control_inbox_sender_reconciliation_started,
    },
  );
  return receipt;
}

export function selectTrustedRelease({
  expectedCommit,
  artifact,
  expectedActiveVersion,
  sourceReceipt,
  readinessReceipt,
  producerReceipt,
  expectedMigrations,
  trustedProducers,
  nowMs = Date.now(),
}) {
  requireGitObject(expectedCommit, "expected release commit");
  requireUuid(expectedActiveVersion, "expected active Worker version");
  if (artifact !== "A" && artifact !== "B") {
    throw new Error("release artifact must be exactly A or B");
  }
  const source = validateSourceReceipt(sourceReceipt, expectedCommit, nowMs);
  const readiness = validateReadinessReceipt(readinessReceipt, {
    expectedCommit,
    artifact,
    expectedActiveVersion,
    nowMs,
  });
  const producerEvidence = verifyDeploymentEvidenceReceipt(
    producerReceipt,
    {
      archiveId: readiness.archive_id,
      artifact,
      artifactBundles: readiness.artifact_bundles,
      expectedCommit,
      expectedDeploymentId: readiness.deployment_id,
      expectedMigrations,
      expectedWorkerVersion: readiness.active_worker_version,
    },
    { trustedProducers, nowMs },
  );
  return {
    format: TRUSTED_RELEASE_SELECTION_FORMAT,
    selection_admitted: true,
    execution_authorized: false,
    execution_performed: false,
    expected_commit: expectedCommit,
    repository_tree: source.repository_tree,
    keyserver_tree: source.keyserver_tree,
    artifact,
    archive_id: readiness.archive_id,
    active_worker_version: readiness.active_worker_version,
    deployment_id: readiness.deployment_id,
    database: readiness.database,
    database_id: readiness.database_id,
    environment: readiness.environment,
    captured_finished_at: readiness.captured_finished_at,
    capability_table_exists: readiness.capability_table_exists,
    markers: readiness.markers,
    route_contract: sourceReceipt.route_contract,
    source_payload_sha256: sourceReceipt.payload_sha256,
    post_deploy_evidence_admitted: true,
    producer_key_id: producerEvidence.producer_key_id,
    producer_identity: producerEvidence.producer_identity,
    producer_run_id: producerEvidence.payload.producer_run_id,
    producer_sequence: producerEvidence.payload.producer_sequence,
    producer_receipt_sha256: producerEvidence.receipt_sha256,
    applied_schema_fingerprint_sha256:
      producerEvidence.payload.database.schema_fingerprint_sha256,
    applied_migrations: producerEvidence.payload.migrations,
  };
}
