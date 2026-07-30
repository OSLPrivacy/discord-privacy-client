import {
  createHash,
  createPublicKey,
  verify as verifySignature,
} from "node:crypto";

export const CANONICAL_ROLLOUT_DATABASE = Object.freeze({
  binding: "DB",
  database_name: "osl-keyserver-prod",
  database_id: "1de837cd-3bf6-4d33-be82-12d358523600",
  config_path: "keyserver-cf/wrangler.toml",
});
export const CANONICAL_ROLLOUT_WORKER_NAME = "oslprivacy-keyserver";
export const CANONICAL_ROLLOUT_MIGRATION =
  "0033_canonical_identity_rollout_authority.sql";
export const CANONICAL_PREKEY_MIGRATION =
  "0034_scheme1_prekey_owner_proofs.sql";
export const CANONICAL_ROLLOUT_EVIDENCE_FORMAT =
  "osl.keyserver.canonical-rollout-predeploy-evidence.v1";
export const CANONICAL_ROLLOUT_EVIDENCE_ENVELOPE_FORMAT =
  "osl.keyserver.canonical-rollout-predeploy-envelope.v1";
export const CANONICAL_ROLLOUT_EVIDENCE_DOMAIN =
  "OSL-KEYSERVER-CANONICAL-ROLLOUT-PREDEPLOY-v1\u0000";
export const CANONICAL_ROLLOUT_ADMISSION_FORMAT =
  "osl.keyserver.canonical-rollout-provisioning-admission.v1";
export const CANONICAL_ROLLOUT_COMPLETION_FORMAT =
  "osl.keyserver.canonical-rollout-completion-evidence.v1";
export const CANONICAL_ROLLOUT_MIGRATION_GATE_FORMAT =
  "osl.keyserver.canonical-rollout-migration-gate.v1";
export const CANONICAL_ROLLOUT_MAX_CAPTURE_MS = 120_000;
export const CANONICAL_ROLLOUT_MAX_EVIDENCE_AGE_MS = 120_000;

// Enrollment is a separately reviewed source change. Caller-authored JSON is
// not authority while this independent producer registry remains empty.
//
// Enrolled 2026-07-29 by owner-authorized release-producer signing-chain
// setup (see /home/liamw/osl-plan/release-producer-key.md for provenance:
// generation time, fingerprint, and custody). The private half never
// leaves /home/liamw/.osl-secrets/release-producer-key/ on the generating
// machine and is not present anywhere in this repository or its history.
export const TRUSTED_CANONICAL_ROLLOUT_EVIDENCE_PRODUCERS =
  Object.freeze({
    "osl-release-producer-20260729-ff7d51bda2c8": Object.freeze({
      identity: "osl-release-producer://liamw",
      public_key_spki_b64:
        "MCowBQYDK2VwAyEABv8sUtAlQ/15L8C2hs+Q2lCOTEFQFPg9bs5e2UFdE5g=",
    }),
  });

export const CANONICAL_ROLLOUT_SOURCE_PATHS = Object.freeze([
  "keyserver-cf/wrangler.toml",
  "keyserver-cf/migrations/0033_canonical_identity_rollout_authority.sql",
  "keyserver-cf/migrations/0034_scheme1_prekey_owner_proofs.sql",
  "keyserver-cf/src/index.ts",
  "keyserver-cf/src/endpoints/register.ts",
  "keyserver-cf/src/endpoints/canonical-identity.ts",
  "keyserver-cf/src/endpoints/pubkeys.ts",
  "keyserver-cf/src/endpoints/prekey-bundle.ts",
  "keyserver-cf/src/endpoints/sender-filter-rollout-root.ts",
  "keyserver-cf/src/lib/identity-authority.ts",
  "keyserver-cf/src/lib/prekey-owner-proof.ts",
  "keyserver-cf/src/lib/db.ts",
  "keyserver-cf/scripts/provision-sender-filter-rollout-genesis.mjs",
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function canonical(value) {
  if (Array.isArray(value)) {
    return `[${value.map(canonical).join(",")}]`;
  }
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) =>
      `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function object(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

function exact(value, fields, label) {
  const item = object(value, label);
  const actual = Object.keys(item).sort();
  const expected = [...fields].sort();
  if (
    actual.length !== expected.length ||
    actual.some((key, index) => key !== expected[index])
  ) {
    throw new Error(`${label} fields are not exact`);
  }
  return item;
}

function gitObject(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{40}$/.test(value) ||
    value === "0".repeat(40)
  ) {
    throw new Error(`${label} must be a nonzero full Git object`);
  }
}

function digest(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{64}$/.test(value) ||
    value === "0".repeat(64)
  ) {
    throw new Error(`${label} must be a nonzero SHA-256`);
  }
}

function uuid(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
      value,
    )
  ) {
    throw new Error(`${label} must be a lowercase UUID`);
  }
}

function timestamp(value, label) {
  if (typeof value !== "string" || !Number.isFinite(Date.parse(value))) {
    throw new Error(`${label} must be an ISO timestamp`);
  }
  return Date.parse(value);
}

function bytes(value, sourcePath) {
  if (Buffer.isBuffer(value)) return value;
  if (value instanceof Uint8Array) return Buffer.from(value);
  if (typeof value === "string") return Buffer.from(value);
  throw new Error(`canonical rollout source is not bytes: ${sourcePath}`);
}

function exactDatabase(value, label) {
  const database = exact(
    value,
    ["binding", "config_path", "database_id", "database_name"],
    label,
  );
  for (const [key, expected] of Object.entries(CANONICAL_ROLLOUT_DATABASE)) {
    if (database[key] !== expected) {
      throw new Error(`${label} ${key} mismatch`);
    }
  }
  return database;
}

function exactSourceAnchor(value) {
  const source = exact(
    value,
    ["commit", "keyserver_tree", "repository_tree"],
    "canonical rollout source anchor",
  );
  gitObject(source.commit, "canonical rollout Worker commit");
  gitObject(source.repository_tree, "canonical rollout repository tree");
  gitObject(source.keyserver_tree, "canonical rollout keyserver tree");
  return source;
}

function canonicalRolloutMigrationPair(value, label) {
  if (!Array.isArray(value)) {
    throw new Error(`${label} must be an array`);
  }
  if (value.length !== 2) {
    throw new Error("migrations 0033/0034 are undeployed or incomplete");
  }
  const rows = value.map((entry) =>
    exact(
      entry,
      ["applied_at", "applied_order", "database_id", "name", "sha256"],
      `${label} row`,
    ));
  const migration = rows.find((row) => row.name === CANONICAL_ROLLOUT_MIGRATION);
  const prekeyMigration = rows.find(
    (row) => row.name === CANONICAL_PREKEY_MIGRATION,
  );
  if (!migration || !prekeyMigration) {
    throw new Error("migrations 0033/0034 are undeployed or incomplete");
  }
  if (
    migration.database_id !== CANONICAL_ROLLOUT_DATABASE.database_id ||
    prekeyMigration.database_id !== CANONICAL_ROLLOUT_DATABASE.database_id
  ) {
    throw new Error("migrations 0033/0034 target the wrong D1 database");
  }
  if (migration.applied_order !== 33 || prekeyMigration.applied_order !== 34) {
    throw new Error("migrations 0033/0034 applied order is not exact");
  }
  digest(migration.sha256, "canonical rollout migration digest");
  digest(prekeyMigration.sha256, "scheme-1 prekey migration digest");
  const migrationApplied = timestamp(
    migration.applied_at,
    "canonical rollout migration application",
  );
  const prekeyMigrationApplied = timestamp(
    prekeyMigration.applied_at,
    "scheme-1 prekey migration application",
  );
  if (prekeyMigrationApplied < migrationApplied) {
    throw new Error("migrations 0033/0034 timestamp order is invalid");
  }
  return { migration, prekeyMigration, migrationApplied, prekeyMigrationApplied };
}

export function validateCanonicalRolloutMigrationGate({
  migrationRows,
  sourceFiles,
  worker,
}) {
  const {
    migration,
    prekeyMigration,
    migrationApplied,
    prekeyMigrationApplied,
  } = canonicalRolloutMigrationPair(
    migrationRows,
    "canonical rollout D1 migration readback",
  );
  if (
    !Array.isArray(sourceFiles) ||
    sourceFiles.length !== CANONICAL_ROLLOUT_SOURCE_PATHS.length
  ) {
    throw new Error("canonical rollout source files are incomplete");
  }
  const migrationFile = sourceFiles.find((file) =>
    file.path.endsWith(`/${CANONICAL_ROLLOUT_MIGRATION}`));
  const prekeyMigrationFile = sourceFiles.find((file) =>
    file.path.endsWith(`/${CANONICAL_PREKEY_MIGRATION}`));
  if (!migrationFile || migrationFile.sha256 !== migration.sha256) {
    throw new Error("migration 0033 readback is not source-bound");
  }
  if (!prekeyMigrationFile || prekeyMigrationFile.sha256 !== prekeyMigration.sha256) {
    throw new Error("migration 0034 readback is not source-bound");
  }

  const observedWorker = exact(
    worker,
    [
      "binding",
      "database_id",
      "deployed_at",
      "name",
    ],
    "canonical rollout Worker activation gate",
  );
  if (
    observedWorker.name !== CANONICAL_ROLLOUT_WORKER_NAME ||
    observedWorker.binding !== CANONICAL_ROLLOUT_DATABASE.binding ||
    observedWorker.database_id !== CANONICAL_ROLLOUT_DATABASE.database_id
  ) {
    throw new Error("canonical rollout Worker is not bound to the exact D1");
  }
  const workerDeployed = timestamp(
    observedWorker.deployed_at,
    "canonical rollout Worker deployment",
  );
  if (
    migrationApplied >= workerDeployed ||
    prekeyMigrationApplied >= workerDeployed
  ) {
    throw new Error("scheme-1 Worker activation before migrations 0033/0034 is refused");
  }
  return Object.freeze({
    format: CANONICAL_ROLLOUT_MIGRATION_GATE_FORMAT,
    migrations_deployed: true,
    migration_order_verified: true,
    source_bound: true,
    worker_activation_precondition_satisfied: true,
    execution_authorized: false,
    migration_execution_authorized: false,
    worker_activation_authorized: false,
    database: { ...CANONICAL_ROLLOUT_DATABASE },
    migration: Object.freeze({ ...migration }),
    prekey_migration: Object.freeze({ ...prekeyMigration }),
    worker: Object.freeze({ ...observedWorker }),
  });
}

export function validateCanonicalRolloutSourceClosure(fileValues) {
  const files = exact(
    fileValues,
    CANONICAL_ROLLOUT_SOURCE_PATHS,
    "canonical rollout source closure",
  );
  const texts = {};
  const result = [];
  for (const sourcePath of CANONICAL_ROLLOUT_SOURCE_PATHS) {
    const sourceBytes = bytes(files[sourcePath], sourcePath);
    if (sourceBytes.length === 0) {
      throw new Error(`canonical rollout source is empty: ${sourcePath}`);
    }
    texts[sourcePath] = sourceBytes.toString("utf8");
    result.push({
      path: sourcePath,
      bytes: sourceBytes.length,
      sha256: sha256(sourceBytes),
    });
  }

  // Parse active TOML lines rather than accepting a security-critical string
  // merely because it survives in a comment or dead helper. Migration and
  // route semantics are exercised through real D1/SELF/main() tests; this
  // function's job is to hash and bind their complete reviewed bytes.
  const wrangler = texts["keyserver-cf/wrangler.toml"];
  const activeLines = wrangler.split(/\r?\n/u)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("#"));
  if (!activeLines.includes(`name = "${CANONICAL_ROLLOUT_WORKER_NAME}"`)) {
    throw new Error("canonical rollout source lacks active exact Worker name");
  }
  const d1Start = activeLines.indexOf("[[d1_databases]]");
  const d1End = activeLines.findIndex(
    (line, index) => index > d1Start && line.startsWith("[["),
  );
  const d1Block = activeLines.slice(
    d1Start,
    d1End === -1 ? activeLines.length : d1End,
  );
  for (const line of [
    'binding = "DB"',
    'database_name = "osl-keyserver-prod"',
    'database_id = "1de837cd-3bf6-4d33-be82-12d358523600"',
    'migrations_dir = "migrations"',
  ]) {
    if (d1Start === -1 || !d1Block.includes(line)) {
      throw new Error(`canonical rollout D1 source lacks active ${line}`);
    }
  }

  return Object.freeze(result);
}

function validatePredeployPayload(value, source, nowMs) {
  const evidence = exact(
    value,
    [
      "capture",
      "database",
      "format",
      "migration",
      "prekey_migration",
      "worker",
    ],
    "canonical rollout predeploy evidence",
  );
  if (evidence.format !== CANONICAL_ROLLOUT_EVIDENCE_FORMAT) {
    throw new Error("canonical rollout predeploy evidence format mismatch");
  }
  exactDatabase(evidence.database, "canonical rollout evidence database");
  const capture = exact(
    evidence.capture,
    ["finished_at", "started_at"],
    "canonical rollout evidence capture",
  );
  const captureStart = timestamp(capture.started_at, "canonical rollout capture start");
  const captureFinish = timestamp(capture.finished_at, "canonical rollout capture finish");
  if (
    captureFinish < captureStart ||
    captureFinish - captureStart > CANONICAL_ROLLOUT_MAX_CAPTURE_MS ||
    nowMs - captureFinish > CANONICAL_ROLLOUT_MAX_EVIDENCE_AGE_MS ||
    captureFinish > nowMs + 10_000
  ) {
    throw new Error("canonical rollout evidence capture is stale or invalid");
  }

  const migration = exact(
    evidence.migration,
    ["applied_at", "applied_order", "database_id", "name", "sha256"],
    "canonical rollout migration evidence",
  );
  if (
    migration.name !== CANONICAL_ROLLOUT_MIGRATION ||
    migration.database_id !== CANONICAL_ROLLOUT_DATABASE.database_id ||
    migration.applied_order !== 33
  ) {
    throw new Error("canonical rollout migration evidence is not exact");
  }
  digest(migration.sha256, "canonical rollout migration digest");
  const migrationApplied = timestamp(
    migration.applied_at,
    "canonical rollout migration application",
  );
  const prekeyMigration = exact(
    evidence.prekey_migration,
    ["applied_at", "applied_order", "database_id", "name", "sha256"],
    "scheme-1 prekey migration evidence",
  );
  if (
    prekeyMigration.name !== CANONICAL_PREKEY_MIGRATION ||
    prekeyMigration.database_id !== CANONICAL_ROLLOUT_DATABASE.database_id ||
    prekeyMigration.applied_order !== 34
  ) {
    throw new Error("scheme-1 prekey migration evidence is not exact");
  }
  digest(prekeyMigration.sha256, "scheme-1 prekey migration digest");
  const prekeyMigrationApplied = timestamp(
    prekeyMigration.applied_at,
    "scheme-1 prekey migration application",
  );
  if (prekeyMigrationApplied < migrationApplied) {
    throw new Error("scheme-1 prekey migration order is invalid");
  }

  const worker = exact(
    evidence.worker,
    [
      "binding",
      "commit",
      "database_id",
      "deployed_at",
      "deployment_id",
      "keyserver_tree",
      "name",
      "provider_observation",
      "repository_tree",
      "version_id",
    ],
    "canonical rollout Worker evidence",
  );
  if (
    worker.name !== CANONICAL_ROLLOUT_WORKER_NAME ||
    worker.binding !== CANONICAL_ROLLOUT_DATABASE.binding ||
    worker.database_id !== CANONICAL_ROLLOUT_DATABASE.database_id ||
    worker.commit !== source.commit ||
    worker.repository_tree !== source.repository_tree ||
    worker.keyserver_tree !== source.keyserver_tree
  ) {
    throw new Error("canonical rollout Worker source or D1 binding mismatch");
  }
  uuid(worker.version_id, "canonical rollout Worker version");
  uuid(worker.deployment_id, "canonical rollout Worker deployment");
  const workerDeployed = timestamp(
    worker.deployed_at,
    "canonical rollout Worker deployment",
  );
  const providerObservation = exact(
    worker.provider_observation,
    [
      "account_fingerprint_sha256",
      "deployment_id",
      "observed_at",
      "provider",
      "script_name",
      "traffic_percent",
      "version_id",
    ],
    "canonical rollout provider Worker observation",
  );
  digest(
    providerObservation.account_fingerprint_sha256,
    "canonical rollout provider account fingerprint",
  );
  uuid(
    providerObservation.version_id,
    "canonical rollout provider Worker version",
  );
  uuid(
    providerObservation.deployment_id,
    "canonical rollout provider Worker deployment",
  );
  const providerObservedAt = timestamp(
    providerObservation.observed_at,
    "canonical rollout provider Worker observation",
  );
  if (
    providerObservation.provider !== "cloudflare-workers-api" ||
    providerObservation.script_name !== CANONICAL_ROLLOUT_WORKER_NAME ||
    providerObservation.version_id !== worker.version_id ||
    providerObservation.deployment_id !== worker.deployment_id ||
    providerObservation.traffic_percent !== 100 ||
    providerObservedAt < workerDeployed ||
    providerObservedAt > captureFinish
  ) {
    throw new Error(
      "canonical rollout provider did not observe the exact Worker at 100% traffic",
    );
  }
  if (
    migrationApplied >= workerDeployed ||
    prekeyMigrationApplied >= workerDeployed
  ) {
    throw new Error("worker-first canonical rollout is refused");
  }
  if (workerDeployed > captureFinish) {
    throw new Error("canonical rollout Worker deployment is not in the capture");
  }
  return evidence;
}

function decodeCanonicalBase64(value, label) {
  if (typeof value !== "string") {
    throw new Error(`${label} must be canonical base64`);
  }
  const decoded = Buffer.from(value, "base64");
  if (decoded.length === 0 || decoded.toString("base64") !== value) {
    throw new Error(`${label} must be canonical base64`);
  }
  return decoded;
}

function validatePredeployEvidence(
  envelopeValue,
  source,
  nowMs,
  trustedProducers,
) {
  const envelope = exact(
    envelopeValue,
    [
      "format",
      "payload",
      "payload_sha256",
      "producer_key_id",
      "signature_b64",
    ],
    "canonical rollout predeploy evidence envelope",
  );
  if (envelope.format !== CANONICAL_ROLLOUT_EVIDENCE_ENVELOPE_FORMAT) {
    throw new Error("canonical rollout evidence envelope format mismatch");
  }
  if (
    typeof envelope.producer_key_id !== "string" ||
    envelope.producer_key_id.length === 0
  ) {
    throw new Error("canonical rollout evidence producer key id is empty");
  }
  const producer = trustedProducers[envelope.producer_key_id];
  if (
    !producer ||
    typeof producer !== "object" ||
    Array.isArray(producer) ||
    Object.keys(producer).sort().join(",") !==
      ["identity", "public_key_spki_b64"].sort().join(",") ||
    typeof producer.identity !== "string" ||
    producer.identity.length === 0
  ) {
    throw new Error(
      "canonical rollout evidence producer is not independently trusted",
    );
  }
  digest(envelope.payload_sha256, "canonical rollout evidence payload");
  const payloadBytes = Buffer.from(canonical(envelope.payload));
  if (sha256(payloadBytes) !== envelope.payload_sha256) {
    throw new Error("canonical rollout evidence payload digest mismatch");
  }
  const publicKey = createPublicKey({
    key: decodeCanonicalBase64(
      producer.public_key_spki_b64,
      "canonical rollout producer public key",
    ),
    format: "der",
    type: "spki",
  });
  const signature = decodeCanonicalBase64(
    envelope.signature_b64,
    "canonical rollout evidence signature",
  );
  if (
    signature.length !== 64 ||
    !verifySignature(
      null,
      Buffer.concat([
        Buffer.from(CANONICAL_ROLLOUT_EVIDENCE_DOMAIN),
        payloadBytes,
      ]),
      publicKey,
      signature,
    )
  ) {
    throw new Error("canonical rollout evidence signature is invalid");
  }
  return {
    envelope,
    producer_identity: producer.identity,
    payload: validatePredeployPayload(envelope.payload, source, nowMs),
  };
}

function receiptPayload(receipt) {
  const {
    payload_sha256: _payloadSha256,
    ...payload
  } = receipt;
  return payload;
}

export function createCanonicalRolloutProvisioningAdmission({
  anchor,
  fileValues,
  evidence,
  nowMs = Date.now(),
  receiptNonce,
  trustedProducers =
    TRUSTED_CANONICAL_ROLLOUT_EVIDENCE_PRODUCERS,
}) {
  if (!Number.isFinite(nowMs)) throw new Error("canonical rollout current time is invalid");
  const source = exactSourceAnchor(anchor);
  const files = validateCanonicalRolloutSourceClosure(fileValues);
  const verifiedEvidence = validatePredeployEvidence(
    evidence,
    source,
    nowMs,
    trustedProducers,
  );
  const checkedEvidence = verifiedEvidence.payload;
  const migrationFile = files.find((entry) =>
    entry.path.endsWith(`/${CANONICAL_ROLLOUT_MIGRATION}`));
  if (!migrationFile || checkedEvidence.migration.sha256 !== migrationFile.sha256) {
    throw new Error("applied migration digest does not match the exact Worker source");
  }
  const prekeyMigrationFile = files.find((entry) =>
    entry.path.endsWith(`/${CANONICAL_PREKEY_MIGRATION}`));
  if (
    !prekeyMigrationFile ||
    checkedEvidence.prekey_migration.sha256 !== prekeyMigrationFile.sha256
  ) {
    throw new Error(
      "applied scheme-1 prekey migration digest does not match exact source",
    );
  }
  uuid(receiptNonce, "canonical rollout admission nonce");
  const payload = {
    format: CANONICAL_ROLLOUT_ADMISSION_FORMAT,
    admission_scope: "scheme-1-root-and-sender-filter-genesis",
    deployment_admitted: false,
    execution_authorized: false,
    provisioning_admitted: true,
    issued_at: new Date(nowMs).toISOString(),
    receipt_nonce: receiptNonce,
    producer_key_id: verifiedEvidence.envelope.producer_key_id,
    producer_identity: verifiedEvidence.producer_identity,
    producer_evidence: verifiedEvidence.envelope,
    source,
    source_files: files,
    database: { ...CANONICAL_ROLLOUT_DATABASE },
    migration: checkedEvidence.migration,
    prekey_migration: checkedEvidence.prekey_migration,
    worker: checkedEvidence.worker,
  };
  return Object.freeze({
    ...payload,
    payload_sha256: sha256(Buffer.from(canonical(payload))),
  });
}

export function validateCanonicalRolloutProvisioningReceipt(
  receiptValue,
  {
    expectedCommit,
    expectedRepositoryTree,
    expectedKeyserverTree,
    nowMs = Date.now(),
    trustedProducers =
      TRUSTED_CANONICAL_ROLLOUT_EVIDENCE_PRODUCERS,
  },
) {
  const receipt = exact(
    receiptValue,
    [
      "admission_scope",
      "database",
      "deployment_admitted",
      "execution_authorized",
      "format",
      "issued_at",
      "migration",
      "prekey_migration",
      "payload_sha256",
      "producer_evidence",
      "producer_identity",
      "producer_key_id",
      "provisioning_admitted",
      "receipt_nonce",
      "source",
      "source_files",
      "worker",
    ],
    "canonical rollout provisioning receipt",
  );
  if (
    receipt.format !== CANONICAL_ROLLOUT_ADMISSION_FORMAT ||
    receipt.admission_scope !== "scheme-1-root-and-sender-filter-genesis" ||
    receipt.deployment_admitted !== false ||
    receipt.execution_authorized !== false ||
    receipt.provisioning_admitted !== true
  ) {
    throw new Error("canonical rollout provisioning receipt disposition mismatch");
  }
  const source = exactSourceAnchor(receipt.source);
  if (
    source.commit !== expectedCommit ||
    source.repository_tree !== expectedRepositoryTree ||
    source.keyserver_tree !== expectedKeyserverTree
  ) {
    throw new Error("canonical rollout provisioning receipt source mismatch");
  }
  exactDatabase(receipt.database, "canonical rollout receipt database");
  const verifiedEvidence = validatePredeployEvidence(
    receipt.producer_evidence,
    source,
    nowMs,
    trustedProducers,
  );
  if (
    receipt.producer_key_id !==
      receipt.producer_evidence.producer_key_id ||
    receipt.producer_identity !== verifiedEvidence.producer_identity ||
    canonical(receipt.migration) !==
      canonical(verifiedEvidence.payload.migration) ||
    canonical(receipt.prekey_migration) !==
      canonical(verifiedEvidence.payload.prekey_migration) ||
    canonical(receipt.worker) !==
      canonical(verifiedEvidence.payload.worker)
  ) {
    throw new Error(
      "canonical rollout producer evidence is not bound to receipt",
    );
  }
  uuid(receipt.receipt_nonce, "canonical rollout receipt nonce");
  digest(receipt.payload_sha256, "canonical rollout receipt digest");
  const issuedAt = timestamp(receipt.issued_at, "canonical rollout receipt issue time");
  if (
    nowMs < issuedAt ||
    nowMs - issuedAt > CANONICAL_ROLLOUT_MAX_EVIDENCE_AGE_MS
  ) {
    throw new Error("canonical rollout provisioning receipt is stale");
  }
  if (sha256(Buffer.from(canonical(receiptPayload(receipt)))) !== receipt.payload_sha256) {
    throw new Error("canonical rollout provisioning receipt digest mismatch");
  }
  const migration = exact(
    receipt.migration,
    ["applied_at", "applied_order", "database_id", "name", "sha256"],
    "canonical rollout receipt migration",
  );
  if (
    migration.name !== CANONICAL_ROLLOUT_MIGRATION ||
    migration.applied_order !== 33 ||
    migration.database_id !== CANONICAL_ROLLOUT_DATABASE.database_id
  ) {
    throw new Error("canonical rollout receipt migration is not exact");
  }
  digest(migration.sha256, "canonical rollout receipt migration");
  const migrationApplied = timestamp(
    migration.applied_at,
    "canonical rollout receipt migration application",
  );
  const prekeyMigration = exact(
    receipt.prekey_migration,
    ["applied_at", "applied_order", "database_id", "name", "sha256"],
    "canonical rollout receipt prekey migration",
  );
  if (
    prekeyMigration.name !== CANONICAL_PREKEY_MIGRATION ||
    prekeyMigration.applied_order !== 34 ||
    prekeyMigration.database_id !== CANONICAL_ROLLOUT_DATABASE.database_id
  ) {
    throw new Error("canonical rollout receipt prekey migration is not exact");
  }
  digest(prekeyMigration.sha256, "canonical rollout receipt prekey migration");
  const prekeyMigrationApplied = timestamp(
    prekeyMigration.applied_at,
    "canonical rollout receipt prekey migration application",
  );
  const worker = exact(
    receipt.worker,
    [
      "binding",
      "commit",
      "database_id",
      "deployed_at",
      "deployment_id",
      "keyserver_tree",
      "name",
      "provider_observation",
      "repository_tree",
      "version_id",
    ],
    "canonical rollout receipt Worker",
  );
  if (
    worker.name !== CANONICAL_ROLLOUT_WORKER_NAME ||
    worker.binding !== CANONICAL_ROLLOUT_DATABASE.binding ||
    worker.database_id !== CANONICAL_ROLLOUT_DATABASE.database_id ||
    worker.commit !== source.commit ||
    worker.repository_tree !== source.repository_tree ||
    worker.keyserver_tree !== source.keyserver_tree
  ) {
    throw new Error("canonical rollout receipt Worker source mismatch");
  }
  uuid(worker.version_id, "canonical rollout receipt Worker version");
  uuid(worker.deployment_id, "canonical rollout receipt Worker deployment");
  const workerDeployed = timestamp(
    worker.deployed_at,
    "canonical rollout receipt Worker deployment",
  );
  if (
    migrationApplied >= workerDeployed ||
    prekeyMigrationApplied < migrationApplied ||
    prekeyMigrationApplied >= workerDeployed ||
    workerDeployed > issuedAt
  ) {
    throw new Error("canonical rollout receipt is worker-first or time-incoherent");
  }
  const providerObservation = exact(
    worker.provider_observation,
    [
      "account_fingerprint_sha256",
      "deployment_id",
      "observed_at",
      "provider",
      "script_name",
      "traffic_percent",
      "version_id",
    ],
    "canonical rollout receipt provider observation",
  );
  digest(
    providerObservation.account_fingerprint_sha256,
    "canonical rollout receipt provider account fingerprint",
  );
  if (
    providerObservation.provider !== "cloudflare-workers-api" ||
    providerObservation.script_name !== CANONICAL_ROLLOUT_WORKER_NAME ||
    providerObservation.version_id !== worker.version_id ||
    providerObservation.deployment_id !== worker.deployment_id ||
    providerObservation.traffic_percent !== 100 ||
    timestamp(
      providerObservation.observed_at,
      "canonical rollout receipt provider observation",
    ) < workerDeployed ||
    timestamp(
      providerObservation.observed_at,
      "canonical rollout receipt provider observation",
    ) > issuedAt
  ) {
    throw new Error(
      "canonical rollout receipt lacks exact provider-observed 100% traffic",
    );
  }
  if (
    !Array.isArray(receipt.source_files) ||
    receipt.source_files.length !== CANONICAL_ROLLOUT_SOURCE_PATHS.length
  ) {
    throw new Error("canonical rollout receipt source evidence is empty");
  }
  const seenPaths = new Set();
  for (const fileValue of receipt.source_files) {
    const file = exact(
      fileValue,
      ["bytes", "path", "sha256"],
      "canonical rollout receipt source file",
    );
    if (
      !CANONICAL_ROLLOUT_SOURCE_PATHS.includes(file.path) ||
      seenPaths.has(file.path) ||
      !Number.isSafeInteger(file.bytes) ||
      file.bytes <= 0
    ) {
      throw new Error("canonical rollout receipt source file is invalid");
    }
    digest(file.sha256, "canonical rollout receipt source file");
    seenPaths.add(file.path);
  }
  const migrationFile = receipt.source_files.find((file) =>
    file.path.endsWith(`/${CANONICAL_ROLLOUT_MIGRATION}`));
  if (!migrationFile || migrationFile.sha256 !== migration.sha256) {
    throw new Error("canonical rollout receipt migration is not source-bound");
  }
  const prekeyMigrationFile = receipt.source_files.find((file) =>
    file.path.endsWith(`/${CANONICAL_PREKEY_MIGRATION}`));
  if (
    !prekeyMigrationFile ||
    prekeyMigrationFile.sha256 !== prekeyMigration.sha256
  ) {
    throw new Error(
      "canonical rollout receipt prekey migration is not source-bound",
    );
  }
  return receipt;
}

export function validateCanonicalRolloutCompletionEvidence({
  receipt,
  evidence: evidenceValue,
}) {
  const evidence = exact(
    evidenceValue,
    [
      "admission_receipt",
      "cas",
      "format",
      "genesis",
      "negative_controls",
      "recovery_manifest",
      "root",
    ],
    "canonical rollout completion evidence",
  );
  if (evidence.format !== CANONICAL_ROLLOUT_COMPLETION_FORMAT) {
    throw new Error("canonical rollout completion evidence format mismatch");
  }
  const admission = exact(
    evidence.admission_receipt,
    ["row_count", "sha256"],
    "canonical rollout admission readback",
  );
  if (admission.row_count !== 1 || admission.sha256 !== receipt.payload_sha256) {
    throw new Error("canonical rollout admission receipt was not consumed exactly once");
  }
  const genesis = exact(
    evidence.genesis,
    [
      "consumed_at_ms",
      "nonce_sha256",
      "provisioned_at_ms",
      "row_count",
      "singleton",
    ],
    "canonical rollout genesis readback",
  );
  digest(genesis.nonce_sha256, "canonical rollout genesis nonce");
  if (
    genesis.row_count !== 1 ||
    genesis.singleton !== 1 ||
    !Number.isSafeInteger(genesis.provisioned_at_ms) ||
    !Number.isSafeInteger(genesis.consumed_at_ms) ||
    genesis.consumed_at_ms < genesis.provisioned_at_ms
  ) {
    throw new Error("canonical rollout genesis readback is empty or invalid");
  }
  const recovery = exact(
    evidence.recovery_manifest,
    [
      "file_mode",
      "manifest_sha256",
      "nonce_sha256",
      "provisioning_admission_sha256",
      "raw_nonce_present",
      "state",
    ],
    "canonical rollout recovery manifest evidence",
  );
  digest(recovery.manifest_sha256, "canonical rollout recovery manifest");
  if (
    recovery.nonce_sha256 !== genesis.nonce_sha256 ||
    recovery.provisioning_admission_sha256 !== receipt.payload_sha256 ||
    recovery.file_mode !== 384 ||
    recovery.raw_nonce_present !== true ||
    recovery.state !== "retained-after-ambiguous-failure"
  ) {
    throw new Error("canonical rollout ambiguous-failure nonce was not retained");
  }
  const root = exact(
    evidence.root,
    [
      "capability_version",
      "identity_bundle_sha256",
      "last_observation_sha256",
      "monotonic_version",
      "provisioned_at_ms",
      "root_ed25519_pub",
      "root_user_id",
      "row_count",
      "singleton",
      "updated_at_ms",
    ],
    "canonical rollout root readback",
  );
  digest(root.identity_bundle_sha256, "canonical rollout root bundle");
  digest(root.last_observation_sha256, "canonical rollout root observation");
  if (
    root.row_count !== 1 ||
    root.singleton !== 1 ||
    typeof root.root_user_id !== "string" ||
    !/^osl1_[a-z2-7]{52}$/.test(root.root_user_id) ||
    typeof root.root_ed25519_pub !== "string" ||
    root.root_ed25519_pub.length !== 44 ||
    !Number.isSafeInteger(root.capability_version) ||
    root.capability_version < 1 ||
    root.monotonic_version !== 2 ||
    !Number.isSafeInteger(root.provisioned_at_ms) ||
    !Number.isSafeInteger(root.updated_at_ms) ||
    root.updated_at_ms < root.provisioned_at_ms
  ) {
    throw new Error("canonical rollout root readback is empty or invalid");
  }
  const cas = exact(
    evidence.cas,
    [
      "applied_changes",
      "expected_monotonic_version",
      "next_monotonic_version",
      "stale_changes",
      "stale_expected_monotonic_version",
    ],
    "canonical rollout CAS evidence",
  );
  if (
    cas.expected_monotonic_version !== 1 ||
    cas.next_monotonic_version !== 2 ||
    cas.applied_changes !== 1 ||
    cas.stale_expected_monotonic_version !== 1 ||
    cas.stale_changes !== 0
  ) {
    throw new Error("canonical rollout monotonic CAS evidence is not exact");
  }
  const controls = exact(
    evidence.negative_controls,
    [
      "delete_genesis_refused",
      "delete_receipt_refused",
      "delete_root_refused",
      "duplicate_genesis_refused",
      "replayed_receipt_refused",
      "reset_root_refused",
      "second_root_refused",
    ],
    "canonical rollout negative controls",
  );
  if (Object.values(controls).some((value) => value !== true)) {
    throw new Error("canonical rollout negative controls are incomplete");
  }
  return Object.freeze({
    format: "osl.keyserver.canonical-rollout-completion-admission.v1",
    deployment_admitted: false,
    execution_authorized: false,
    completion_evidence_admitted: true,
    source_commit: receipt.source.commit,
    source_tree: receipt.source.repository_tree,
    keyserver_tree: receipt.source.keyserver_tree,
    admission_receipt_sha256: receipt.payload_sha256,
    evidence_sha256: sha256(Buffer.from(canonical(evidence))),
  });
}

if (process.env.OSL_CANONICAL_ROLLOUT_CONTRACT_SELFTEST === "1") {
  const { default: assert } = await import("node:assert/strict");
  const { test } = await import("node:test");

  const row33 = Object.freeze({
    name: CANONICAL_ROLLOUT_MIGRATION,
    sha256: "1".repeat(64),
    applied_order: 33,
    applied_at: "2026-07-27T19:59:51.000Z",
    database_id: CANONICAL_ROLLOUT_DATABASE.database_id,
  });
  const row34 = Object.freeze({
    name: CANONICAL_PREKEY_MIGRATION,
    sha256: "2".repeat(64),
    applied_order: 34,
    applied_at: "2026-07-27T19:59:52.000Z",
    database_id: CANONICAL_ROLLOUT_DATABASE.database_id,
  });
  const sourceFiles = Object.freeze(
    CANONICAL_ROLLOUT_SOURCE_PATHS.map((sourcePath) => Object.freeze({
      path: sourcePath,
      bytes: 1,
      sha256: sourcePath.endsWith(`/${CANONICAL_ROLLOUT_MIGRATION}`)
        ? row33.sha256
        : sourcePath.endsWith(`/${CANONICAL_PREKEY_MIGRATION}`)
          ? row34.sha256
          : "3".repeat(64),
    })),
  );
  const worker = Object.freeze({
    name: CANONICAL_ROLLOUT_WORKER_NAME,
    deployed_at: "2026-07-27T19:59:55.000Z",
    binding: CANONICAL_ROLLOUT_DATABASE.binding,
    database_id: CANONICAL_ROLLOUT_DATABASE.database_id,
  });

  test("same-file b46 gate refuses absent 0033/0034 rows", () => {
    assert.throws(
      () => validateCanonicalRolloutMigrationGate({
        migrationRows: [],
        sourceFiles,
        worker,
      }),
      /0033\/0034 are undeployed/,
    );
  });

  test("same-file b46 gate verifies order without authorizing execution", () => {
    const gate = validateCanonicalRolloutMigrationGate({
      migrationRows: [row33, row34],
      sourceFiles,
      worker,
    });
    assert.equal(gate.format, CANONICAL_ROLLOUT_MIGRATION_GATE_FORMAT);
    assert.equal(gate.migration_order_verified, true);
    assert.equal(gate.execution_authorized, false);
    assert.equal(gate.migration_execution_authorized, false);
    assert.equal(gate.worker_activation_authorized, false);
  });
}
