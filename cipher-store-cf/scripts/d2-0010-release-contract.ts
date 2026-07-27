/**
 * D2 migration-0010 evidence contract.
 *
 * Local Workerd evidence and production authorization are deliberately
 * different formats. A local callback proves only the local branches that were
 * actually exercised. Production admission requires Ed25519 signatures from a
 * fixed, compiled producer registry over provider observations and every
 * individual probe receipt. The registry is intentionally empty until an
 * independently reviewed producer is provisioned, so production admission is
 * currently blocked.
 */

export const D2_LOCAL_EVIDENCE_FORMAT =
  "osl.cipher-store.d2-migration-0010-local-evidence.v2";
export const D2_LOCAL_RECEIPT_FORMAT =
  "osl.cipher-store.d2-migration-0010-local-receipt.v2";
export const D2_PRODUCTION_EVIDENCE_FORMAT =
  "osl.cipher-store.d2-migration-0010-production-evidence.v2";
export const D2_PRODUCTION_RECEIPT_FORMAT =
  "osl.cipher-store.d2-migration-0010-production-receipt.v2";
export const D2_SIGNED_STATEMENT_FORMAT =
  "osl.cipher-store.d2-migration-0010-signed-statement.v1";
export const D2_PROBE_FORMAT =
  "osl.cipher-store.d2-migration-0010-production-probe.v1";
export const D2_RELEASE_COMMIT =
  "3938a73caaed7cd5453fb3595d0270cf74ade998";
export const D2_RELEASE_TREE =
  "37083f2616e7e7f8245e2c0efa2740edfdd85fd2";
export const D2_RELEASE_SOURCE_SHA256 =
  "1031d2fe50caafb726c64518c1d5b70bddcaa2375472a2b0af4881c977bc3e00";
export const D2_MIGRATION_0010_SHA256 =
  "a545f989172c32c8f5f5c78754b4eda2f045778643cbb86c9eb81f22be2f6636";
export const D2_DATABASE_ID = "be3d31f1-f6b4-4d6e-8ede-74514950b9e2";
export const D2_DATABASE_NAME = "osl-cipher-store-prod";
export const D2_R2_BUCKET = "osl-cipher-attachments-prod";
export const D2_WORKER_NAME = "oslprivacy-cipher-store";
export const D2_CRON = "*/5 * * * *";
export const D2_CYCLE_MARKER = "[attachment-sweep-cycle] complete";
export const D2_RECOVERY_MARKER =
  "osl.cipher-store.continuous-predecessor-recovery.v1";

export const D2_RELEASE_SOURCE_FILES = [
  "migrations/0001_init.sql",
  "migrations/0002_fetch_token.sql",
  "migrations/0003_r2_attachments.sql",
  "migrations/0004_attachment_capability_digests_and_quota.sql",
  "migrations/0005_view_once_links.sql",
  "migrations/0006_session_budget_and_atomic_rate_counters.sql",
  "migrations/0007_link_grant_consumption.sql",
  "migrations/0008_attachment_sweep_claims.sql",
  "migrations/0009_predecessor_completing_adoption.sql",
  "migrations/0010_continuous_predecessor_recovery.sql",
  "package-lock.json",
  "package.json",
  "src/endpoints/attachment.ts",
  "src/endpoints/blob.ts",
  "src/endpoints/healthz.ts",
  "src/endpoints/link.ts",
  "src/env.ts",
  "src/index.ts",
  "src/lib/attachment-limits.ts",
  "src/lib/attachment-sweep-claims.ts",
  "src/lib/blob-limits.ts",
  "src/lib/d2-proof-contract.d.ts",
  "src/lib/d2-proof-contract.js",
  "src/lib/digest.ts",
  "src/lib/http.ts",
  "src/lib/id.ts",
  "src/lib/landing.ts",
  "src/lib/link-grant.ts",
  "src/lib/rate-limit.ts",
  "src/lib/sweep.ts",
  "wrangler.toml",
] as const;

export const D2_REQUIRED_MIGRATIONS = [
  "0001_init.sql",
  "0002_fetch_token.sql",
  "0003_r2_attachments.sql",
  "0004_attachment_capability_digests_and_quota.sql",
  "0005_view_once_links.sql",
  "0006_session_budget_and_atomic_rate_counters.sql",
  "0007_link_grant_consumption.sql",
  "0008_attachment_sweep_claims.sql",
  "0009_predecessor_completing_adoption.sql",
  "0010_continuous_predecessor_recovery.sql",
] as const;

export const D2_PROBE_KINDS = [
  "legacy-no-object",
  "exact-size",
  "wrong-size",
  "no-such-upload",
  "unknown-abort",
  "crash-retry",
  "cleanup",
  "rollback-refusal",
] as const;
export type D2ProbeKind = (typeof D2_PROBE_KINDS)[number];
export type D2ProducerRole = "cloudflare-provider" | "production-probe";

export interface D2TrustedProducer {
  public_key_raw_base64url: string;
  account_sha256: string;
  roles: readonly D2ProducerRole[];
}

/**
 * Production authority is code-owned, never caller supplied. It stays empty
 * until the producer key and its account binding receive independent review.
 */
export const D2_TRUSTED_PRODUCERS: Readonly<
  Record<string, D2TrustedProducer>
> = Object.freeze({});

interface ExactSource {
  commit_sha: string;
  tree_sha: string;
  manifest_sha256: string;
}

export interface D2LocalEvidence {
  format: typeof D2_LOCAL_EVIDENCE_FORMAT;
  environment: "local";
  source: ExactSource;
  runtime: {
    engine: "workerd";
    invocation: "manual";
    scheduled_callback_observed: true;
    natural_cron_observation: "unknown";
    marker: typeof D2_CYCLE_MARKER;
  };
  migration: {
    applied_migrations: string[];
    migration_0010_sha256: string;
    recovery_marker: {
      format: string;
      max_claims_per_cycle: number;
    };
    claim_columns: string[];
  };
  witnesses: {
    legacy_no_object: LegacyNoObjectOutcome;
    exact_size: ExactSizeOutcome;
    wrong_size: WrongSizeOutcome;
  };
  cleanup: {
    created_rows: number;
    created_objects: number;
    remaining_rows: number;
    remaining_objects: number;
  };
}

export interface D2LocalReceipt {
  format: typeof D2_LOCAL_RECEIPT_FORMAT;
  verdict: "local-runtime-evidence-only";
  environment: "local";
  source: {
    commit_sha: typeof D2_RELEASE_COMMIT;
    tree_sha: typeof D2_RELEASE_TREE;
    manifest_sha256: typeof D2_RELEASE_SOURCE_SHA256;
  };
  invocation: "manual";
  natural_cron_observation: "unknown";
  executed_witnesses: [
    "legacy-no-object",
    "exact-size",
    "wrong-size",
  ];
  cleanup: {
    remaining_rows: 0;
    remaining_objects: 0;
  };
  production_authorized: false;
}

interface LegacyNoObjectOutcome {
  count: number;
  created_after_0009_cutoff: boolean;
  unlineaged: boolean;
  state_before: string;
  expected_size_bytes: number;
  head_before: string;
  abort_outcome: string;
  post_abort_head: string;
  row_after: string;
  quota_after: string;
}

interface ExactSizeOutcome {
  count: number;
  expected_size_bytes: number;
  observed_size_bytes: number;
  head_before: string;
  abort_calls: number;
  delete_calls: number;
  row_after: string;
  object_after: string;
  quota_after: string;
}

interface WrongSizeOutcome {
  count: number;
  expected_size_bytes: number;
  observed_size_bytes: number;
  abort_order: number;
  delete_order: number;
  absence_cas_order: number;
  post_delete_head: string;
  row_after: string;
  quota_after: string;
}

export interface D2SignedStatement<T = unknown> {
  format: typeof D2_SIGNED_STATEMENT_FORMAT;
  producer_id: string;
  algorithm: "Ed25519";
  payload: T;
  signature_base64url: string;
}

interface ProductionBinding {
  source: ExactSource;
  account_sha256: string;
  database_id: string;
  worker_version_id: string;
  r2_bucket_name: string;
}

export interface D2ProductionDeploymentObservation {
  format: "osl.cipher-store.d2-provider-observation.v1";
  environment: "production";
  observation_source: "cloudflare-authoritative-export";
  source: ExactSource;
  account_sha256: string;
  d1: {
    database_id: string;
    database_name: string;
    observed_at_ms: number;
    applied_migrations: string[];
    migration_0010_sha256: string;
    recovery_marker_format: string;
    max_claims_per_cycle: number;
    claim_columns: string[];
  };
  worker: {
    worker_name: string;
    version_id: string;
    activated_at_ms: number;
    traffic_percentage: number;
    source: ExactSource;
    handlers: string[];
    d1_binding: string;
    d1_database_id: string;
    r2_binding: string;
    r2_bucket_name: string;
  };
  r2: {
    binding: string;
    bucket_name: string;
  };
  cron: {
    configured: string;
    natural_trigger_observation: "observed";
    observation_source: "cloudflare-provider-event";
    worker_version_id: string;
    scheduled_time_ms: number;
    event_time_ms: number;
    outcome: "ok";
    marker: typeof D2_CYCLE_MARKER;
  };
}

export interface D2ProductionProbe {
  format: typeof D2_PROBE_FORMAT;
  environment: "production";
  probe_id: string;
  kind: D2ProbeKind;
  observed_at_ms: number;
  transcript_sha256: string;
  binding: ProductionBinding;
  outcome: Record<string, unknown>;
}

export interface D2ProductionEvidence {
  format: typeof D2_PRODUCTION_EVIDENCE_FORMAT;
  environment: "production";
  provider_observation: D2SignedStatement<D2ProductionDeploymentObservation>;
  probe_receipts: D2SignedStatement<D2ProductionProbe>[];
}

export interface D2ProductionReceipt {
  format: typeof D2_PRODUCTION_RECEIPT_FORMAT;
  verdict: "production-release-authorized";
  environment: "production";
  source: {
    commit_sha: typeof D2_RELEASE_COMMIT;
    tree_sha: typeof D2_RELEASE_TREE;
    manifest_sha256: typeof D2_RELEASE_SOURCE_SHA256;
  };
  account_sha256: string;
  d1: {
    database_id: typeof D2_DATABASE_ID;
    migration_observed_at_ms: number;
  };
  worker: {
    version_id: string;
    activated_at_ms: number;
    traffic_percentage: 100;
  };
  r2: {
    bucket_name: typeof D2_R2_BUCKET;
  };
  cron: {
    natural_trigger_observation: "observed";
    scheduled_time_ms: number;
    event_time_ms: number;
  };
  producer_ids: string[];
  signed_statement_sha256: string[];
  witness_counts: Record<D2ProbeKind, 1>;
  production_authorized: true;
}

export interface D2TestOnlyProductionValidation {
  format: "osl.cipher-store.d2-production-contract-test-result.v1";
  verdict: "test-only-signature-contract-valid";
  production_authorized: false;
  witness_kinds: D2ProbeKind[];
}

const SHA256_RE = /^[0-9a-f]{64}$/;
const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const PROBE_ID_RE = /^[0-9a-f]{32}$/;
const BASE64URL_RE = /^[A-Za-z0-9_-]+$/;

function fail(message: string): never {
  throw new Error(`D2 migration-0010 release contract: ${message}`);
}

function objectValue(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be an object`);
  }
  return value as Record<string, unknown>;
}

function exactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
  label: string,
): void {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length
    || actual.some((key, index) => key !== wanted[index])
  ) {
    fail(`${label} has unexpected or missing fields`);
  }
}

function exactArray(
  value: unknown,
  expected: readonly string[],
  label: string,
): void {
  if (
    !Array.isArray(value)
    || value.length !== expected.length
    || value.some((entry, index) => entry !== expected[index])
  ) {
    fail(`${label} is missing, stale, reordered, or ambiguous`);
  }
}

function positiveInt(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) <= 0) {
    fail(`${label} must be a positive safe integer`);
  }
  return value as number;
}

function nonnegativeInt(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    fail(`${label} must be a non-negative safe integer`);
  }
  return value as number;
}

function exactSource(value: unknown, label: string): ExactSource {
  const source = objectValue(value, label);
  exactKeys(source, ["commit_sha", "tree_sha", "manifest_sha256"], label);
  if (
    source.commit_sha !== D2_RELEASE_COMMIT
    || source.tree_sha !== D2_RELEASE_TREE
    || source.manifest_sha256 !== D2_RELEASE_SOURCE_SHA256
  ) {
    fail(`${label} does not match the exact reviewed commit/tree/source digest`);
  }
  return source as unknown as ExactSource;
}

function validateMigration(value: unknown): number | undefined {
  const migration = objectValue(value, "migration evidence");
  const local = !Object.hasOwn(migration, "observed_at_ms");
  exactKeys(
    migration,
    local
      ? [
        "applied_migrations",
        "migration_0010_sha256",
        "recovery_marker",
        "claim_columns",
      ]
      : [
        "database_id",
        "database_name",
        "observed_at_ms",
        "applied_migrations",
        "migration_0010_sha256",
        "recovery_marker_format",
        "max_claims_per_cycle",
        "claim_columns",
      ],
    "migration evidence",
  );
  exactArray(
    migration.applied_migrations,
    D2_REQUIRED_MIGRATIONS,
    "applied migrations",
  );
  if (migration.migration_0010_sha256 !== D2_MIGRATION_0010_SHA256) {
    fail("migration 0010 is missing or stale");
  }
  exactArray(
    migration.claim_columns,
    ["claim_origin", "storage_fence_state"],
    "migration claim columns",
  );
  if (local) {
    const marker = objectValue(migration.recovery_marker, "recovery marker");
    exactKeys(marker, ["format", "max_claims_per_cycle"], "recovery marker");
    if (
      marker.format !== D2_RECOVERY_MARKER
      || marker.max_claims_per_cycle !== 100
    ) {
      fail("migration 0010 recovery marker is missing or malformed");
    }
    return undefined;
  }
  if (
    migration.database_id !== D2_DATABASE_ID
    || migration.database_name !== D2_DATABASE_NAME
    || migration.recovery_marker_format !== D2_RECOVERY_MARKER
    || migration.max_claims_per_cycle !== 100
  ) {
    fail("production D1 identity or recovery marker is wrong");
  }
  return positiveInt(migration.observed_at_ms, "migration observation time");
}

function validateLegacy(value: unknown): void {
  const witness = objectValue(value, "legacy no-object witness");
  exactKeys(witness, [
    "count",
    "created_after_0009_cutoff",
    "unlineaged",
    "state_before",
    "expected_size_bytes",
    "head_before",
    "abort_outcome",
    "post_abort_head",
    "row_after",
    "quota_after",
  ], "legacy no-object witness");
  if (
    witness.count !== 1
    || witness.created_after_0009_cutoff !== true
    || witness.unlineaged !== true
    || witness.state_before !== "completing"
    || positiveInt(witness.expected_size_bytes, "legacy expected size") <= 0
    || witness.head_before !== "absent"
    || witness.abort_outcome !== "succeeded"
    || witness.post_abort_head !== "absent"
    || witness.row_after !== "removed"
    || witness.quota_after !== "released"
  ) {
    fail("legacy no-object witness is empty or unsafe");
  }
}

function validateExact(value: unknown): void {
  const witness = objectValue(value, "exact-size witness");
  exactKeys(witness, [
    "count",
    "expected_size_bytes",
    "observed_size_bytes",
    "head_before",
    "abort_calls",
    "delete_calls",
    "row_after",
    "object_after",
    "quota_after",
  ], "exact-size witness");
  const expected = positiveInt(witness.expected_size_bytes, "exact expected size");
  const observed = positiveInt(witness.observed_size_bytes, "exact observed size");
  if (
    witness.count !== 1
    || expected !== observed
    || witness.head_before !== "exact"
    || witness.abort_calls !== 0
    || witness.delete_calls !== 0
    || witness.row_after !== "ready"
    || witness.object_after !== "retained"
    || witness.quota_after !== "retained"
  ) {
    fail("exact-size witness is empty or reachable ciphertext was not retained");
  }
}

function validateWrong(value: unknown): void {
  const witness = objectValue(value, "wrong-size witness");
  exactKeys(witness, [
    "count",
    "expected_size_bytes",
    "observed_size_bytes",
    "abort_order",
    "delete_order",
    "absence_cas_order",
    "post_delete_head",
    "row_after",
    "quota_after",
  ], "wrong-size witness");
  const expected = positiveInt(witness.expected_size_bytes, "wrong expected size");
  const observed = positiveInt(witness.observed_size_bytes, "wrong observed size");
  if (
    witness.count !== 1
    || expected === observed
    || witness.abort_order !== 1
    || witness.delete_order !== 2
    || witness.absence_cas_order !== 3
    || witness.post_delete_head !== "absent"
    || witness.row_after !== "removed"
    || witness.quota_after !== "released"
  ) {
    fail("wrong-size witness is empty or cleanup ordering is unsafe");
  }
}

export function verifyD2Migration0010LocalEvidence(
  input: unknown,
): D2LocalReceipt {
  const evidence = objectValue(input, "local evidence");
  exactKeys(
    evidence,
    [
      "format",
      "environment",
      "source",
      "runtime",
      "migration",
      "witnesses",
      "cleanup",
    ],
    "local evidence",
  );
  if (
    evidence.format !== D2_LOCAL_EVIDENCE_FORMAT
    || evidence.environment !== "local"
  ) {
    fail("local evidence must identify environment=local");
  }
  exactSource(evidence.source, "local source");
  const runtime = objectValue(evidence.runtime, "local runtime");
  exactKeys(runtime, [
    "engine",
    "invocation",
    "scheduled_callback_observed",
    "natural_cron_observation",
    "marker",
  ], "local runtime");
  if (
    runtime.engine !== "workerd"
    || runtime.invocation !== "manual"
    || runtime.scheduled_callback_observed !== true
    || runtime.natural_cron_observation !== "unknown"
    || runtime.marker !== D2_CYCLE_MARKER
  ) {
    fail("local callback must remain manual and natural cron must remain unknown");
  }
  validateMigration(evidence.migration);
  const witnesses = objectValue(evidence.witnesses, "local witnesses");
  exactKeys(
    witnesses,
    ["legacy_no_object", "exact_size", "wrong_size"],
    "local witnesses",
  );
  validateLegacy(witnesses.legacy_no_object);
  validateExact(witnesses.exact_size);
  validateWrong(witnesses.wrong_size);
  const cleanup = objectValue(evidence.cleanup, "local cleanup");
  exactKeys(
    cleanup,
    ["created_rows", "created_objects", "remaining_rows", "remaining_objects"],
    "local cleanup",
  );
  if (
    positiveInt(cleanup.created_rows, "created local rows") !== 3
    || positiveInt(cleanup.created_objects, "created local objects") !== 2
    || nonnegativeInt(cleanup.remaining_rows, "remaining local rows") !== 0
    || nonnegativeInt(cleanup.remaining_objects, "remaining local objects") !== 0
  ) {
    fail("local cleanup is empty, incomplete, or ambiguous");
  }
  return {
    format: D2_LOCAL_RECEIPT_FORMAT,
    verdict: "local-runtime-evidence-only",
    environment: "local",
    source: {
      commit_sha: D2_RELEASE_COMMIT,
      tree_sha: D2_RELEASE_TREE,
      manifest_sha256: D2_RELEASE_SOURCE_SHA256,
    },
    invocation: "manual",
    natural_cron_observation: "unknown",
    executed_witnesses: [
      "legacy-no-object",
      "exact-size",
      "wrong-size",
    ],
    cleanup: { remaining_rows: 0, remaining_objects: 0 },
    production_authorized: false,
  };
}

function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const object = value as Record<string, unknown>;
  return `{${Object.keys(object).sort().map((key) => (
    `${JSON.stringify(key)}:${canonicalJson(object[key])}`
  )).join(",")}}`;
}

function base64urlBytes(value: string, label: string): Uint8Array {
  if (!BASE64URL_RE.test(value) || value.includes("=")) {
    fail(`${label} is not strict base64url`);
  }
  const standard = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = standard.padEnd(Math.ceil(standard.length / 4) * 4, "=");
  return new Uint8Array(Buffer.from(padded, "base64"));
}

async function sha256Hex(value: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(value),
  );
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

async function verifySigned<T>(
  input: unknown,
  registry: Readonly<Record<string, D2TrustedProducer>>,
  role: D2ProducerRole,
  label: string,
): Promise<{ envelope: D2SignedStatement<T>; producer: D2TrustedProducer }> {
  const envelope = objectValue(input, label);
  exactKeys(
    envelope,
    ["format", "producer_id", "algorithm", "payload", "signature_base64url"],
    label,
  );
  if (
    envelope.format !== D2_SIGNED_STATEMENT_FORMAT
    || envelope.algorithm !== "Ed25519"
    || typeof envelope.producer_id !== "string"
  ) {
    fail(`${label} format or algorithm is invalid`);
  }
  const producer = registry[envelope.producer_id];
  if (!producer || !producer.roles.includes(role)) {
    fail(`${label} producer is not in the fixed trusted registry for ${role}`);
  }
  if (!SHA256_RE.test(producer.account_sha256)) {
    fail(`${label} producer account binding is malformed`);
  }
  const publicKey = base64urlBytes(
    producer.public_key_raw_base64url,
    `${label} public key`,
  );
  const signature = base64urlBytes(
    String(envelope.signature_base64url),
    `${label} signature`,
  );
  if (publicKey.byteLength !== 32 || signature.byteLength !== 64) {
    fail(`${label} Ed25519 key or signature length is invalid`);
  }
  const key = await crypto.subtle.importKey(
    "raw",
    publicKey,
    { name: "Ed25519" },
    false,
    ["verify"],
  );
  const payload = canonicalJson(envelope.payload);
  const message = new TextEncoder().encode(
    `${D2_SIGNED_STATEMENT_FORMAT}\0${payload}`,
  );
  if (!await crypto.subtle.verify("Ed25519", key, signature, message)) {
    fail(`${label} signature is invalid`);
  }
  return {
    envelope: envelope as unknown as D2SignedStatement<T>,
    producer,
  };
}

function validateProvider(
  input: unknown,
  producer: D2TrustedProducer,
): D2ProductionDeploymentObservation {
  const observation = objectValue(input, "provider observation payload");
  exactKeys(observation, [
    "format",
    "environment",
    "observation_source",
    "source",
    "account_sha256",
    "d1",
    "worker",
    "r2",
    "cron",
  ], "provider observation payload");
  if (
    observation.format !== "osl.cipher-store.d2-provider-observation.v1"
    || observation.environment !== "production"
    || observation.observation_source !== "cloudflare-authoritative-export"
    || observation.account_sha256 !== producer.account_sha256
  ) {
    fail("provider observation is synthetic, nonproduction, or for the wrong account");
  }
  exactSource(observation.source, "provider release source");
  const migrationObservedAt = validateMigration(observation.d1);
  const worker = objectValue(observation.worker, "provider Worker observation");
  exactKeys(worker, [
    "worker_name",
    "version_id",
    "activated_at_ms",
    "traffic_percentage",
    "source",
    "handlers",
    "d1_binding",
    "d1_database_id",
    "r2_binding",
    "r2_bucket_name",
  ], "provider Worker observation");
  if (
    worker.worker_name !== D2_WORKER_NAME
    || typeof worker.version_id !== "string"
    || !UUID_RE.test(worker.version_id)
    || worker.traffic_percentage !== 100
    || worker.d1_binding !== "DB"
    || worker.d1_database_id !== D2_DATABASE_ID
    || worker.r2_binding !== "ATTACHMENTS"
    || worker.r2_bucket_name !== D2_R2_BUCKET
  ) {
    fail("provider Worker identity, traffic, D1, or R2 binding is wrong");
  }
  exactSource(worker.source, "provider Worker source");
  exactArray(worker.handlers, ["fetch", "scheduled"], "provider Worker handlers");
  const activatedAt = positiveInt(worker.activated_at_ms, "Worker activation");
  if (migrationObservedAt === undefined || migrationObservedAt >= activatedAt) {
    fail("Worker activation was not fenced behind migration 0010");
  }
  const r2 = objectValue(observation.r2, "provider R2 observation");
  exactKeys(r2, ["binding", "bucket_name"], "provider R2 observation");
  if (r2.binding !== "ATTACHMENTS" || r2.bucket_name !== D2_R2_BUCKET) {
    fail("provider R2 bucket is wrong");
  }
  const cron = objectValue(observation.cron, "provider cron observation");
  exactKeys(cron, [
    "configured",
    "natural_trigger_observation",
    "observation_source",
    "worker_version_id",
    "scheduled_time_ms",
    "event_time_ms",
    "outcome",
    "marker",
  ], "provider cron observation");
  const scheduledAt = positiveInt(cron.scheduled_time_ms, "natural cron time");
  const eventAt = positiveInt(cron.event_time_ms, "natural cron event time");
  if (
    cron.configured !== D2_CRON
    || cron.natural_trigger_observation !== "observed"
    || cron.observation_source !== "cloudflare-provider-event"
    || cron.worker_version_id !== worker.version_id
    || scheduledAt < activatedAt
    || eventAt < scheduledAt
    || cron.outcome !== "ok"
    || cron.marker !== D2_CYCLE_MARKER
  ) {
    fail("natural scheduled trigger is absent, synthetic, stale, or mismatched");
  }
  return observation as unknown as D2ProductionDeploymentObservation;
}

function validateBinding(
  value: unknown,
  provider: D2ProductionDeploymentObservation,
  producer: D2TrustedProducer,
): void {
  const binding = objectValue(value, "probe binding");
  exactKeys(binding, [
    "source",
    "account_sha256",
    "database_id",
    "worker_version_id",
    "r2_bucket_name",
  ], "probe binding");
  exactSource(binding.source, "probe source");
  if (
    binding.account_sha256 !== producer.account_sha256
    || binding.account_sha256 !== provider.account_sha256
    || binding.database_id !== D2_DATABASE_ID
    || binding.worker_version_id !== provider.worker.version_id
    || binding.r2_bucket_name !== D2_R2_BUCKET
  ) {
    fail("probe receipt is not bound to the authorized deployment");
  }
}

function validateProbeOutcome(kind: D2ProbeKind, value: unknown): void {
  if (kind === "legacy-no-object") return validateLegacy(value);
  if (kind === "exact-size") return validateExact(value);
  if (kind === "wrong-size") return validateWrong(value);
  const outcome = objectValue(value, `${kind} outcome`);
  if (kind === "no-such-upload") {
    exactKeys(outcome, [
      "count",
      "discriminator",
      "value",
      "post_abort_head",
      "decision",
    ], "NoSuchUpload outcome");
    if (
      outcome.count !== 1
      || !["code", "name"].includes(String(outcome.discriminator))
      || outcome.value !== "NoSuchUpload"
      || outcome.post_abort_head !== "absent"
      || outcome.decision !== "resume-idempotently"
    ) {
      fail("NoSuchUpload production witness is empty or widened");
    }
    return;
  }
  if (kind === "unknown-abort") {
    exactKeys(outcome, [
      "count",
      "discriminator",
      "value",
      "decision",
      "delete_calls",
      "metadata_after",
      "quota_after",
    ], "unknown-abort outcome");
    if (
      outcome.count !== 1
      || !["code", "name"].includes(String(outcome.discriminator))
      || typeof outcome.value !== "string"
      || outcome.value.length === 0
      || outcome.value === "NoSuchUpload"
      || outcome.decision !== "retain"
      || outcome.delete_calls !== 0
      || outcome.metadata_after !== "retained"
      || outcome.quota_after !== "retained"
    ) {
      fail("unknown-abort production witness did not retain ambiguity");
    }
    return;
  }
  if (kind === "crash-retry") {
    exactKeys(outcome, [
      "count",
      "absence_fence_persisted",
      "lease_version_increased",
      "final_cleanup",
    ], "crash-retry outcome");
    if (
      outcome.count !== 1
      || outcome.absence_fence_persisted !== true
      || outcome.lease_version_increased !== true
      || outcome.final_cleanup !== "completed"
    ) {
      fail("crash-retry production witness is empty or incomplete");
    }
    return;
  }
  if (kind === "cleanup") {
    exactKeys(outcome, [
      "count",
      "covered_probe_ids",
      "created_rows",
      "created_objects",
      "remaining_rows",
      "remaining_objects",
      "remaining_multipart_uploads",
    ], "cleanup outcome");
    if (
      outcome.count !== 1
      || !Array.isArray(outcome.covered_probe_ids)
      || outcome.covered_probe_ids.length !== 6
      || new Set(outcome.covered_probe_ids).size !== 6
      || positiveInt(outcome.created_rows, "production created rows") < 3
      || positiveInt(outcome.created_objects, "production created objects") < 2
      || nonnegativeInt(outcome.remaining_rows, "production remaining rows") !== 0
      || nonnegativeInt(outcome.remaining_objects, "production remaining objects") !== 0
      || nonnegativeInt(
        outcome.remaining_multipart_uploads,
        "production remaining multipart uploads",
      ) !== 0
    ) {
      fail("production cleanup is empty, fabricated, or incomplete");
    }
    return;
  }
  exactKeys(outcome, [
    "count",
    "requested_target_version_id",
    "target_source_digest_sha256",
    "decision",
    "reason",
    "mutations_performed",
  ], "rollback-refusal outcome");
  if (
    outcome.count !== 1
    || typeof outcome.requested_target_version_id !== "string"
    || !UUID_RE.test(outcome.requested_target_version_id)
    || typeof outcome.target_source_digest_sha256 !== "string"
    || !SHA256_RE.test(outcome.target_source_digest_sha256)
    || outcome.target_source_digest_sha256 === D2_RELEASE_SOURCE_SHA256
    || outcome.decision !== "refused"
    || outcome.reason !== "migration-0010-before-worker"
    || outcome.mutations_performed !== 0
  ) {
    fail("rollback refusal is empty, ambiguous, or mutating");
  }
}

async function validateProduction(
  input: unknown,
  registry: Readonly<Record<string, D2TrustedProducer>>,
): Promise<{
  provider: D2ProductionDeploymentObservation;
  providerId: string;
  probes: D2ProductionProbe[];
  producerIds: string[];
  statementDigests: string[];
}> {
  if (Object.keys(registry).length === 0) {
    fail("no trusted production producers are configured; deployment is blocked");
  }
  const evidence = objectValue(input, "production evidence");
  exactKeys(
    evidence,
    ["format", "environment", "provider_observation", "probe_receipts"],
    "production evidence",
  );
  if (
    evidence.format !== D2_PRODUCTION_EVIDENCE_FORMAT
    || evidence.environment !== "production"
  ) {
    fail("production evidence format or environment is invalid");
  }
  const providerSigned = await verifySigned<D2ProductionDeploymentObservation>(
    evidence.provider_observation,
    registry,
    "cloudflare-provider",
    "provider observation",
  );
  const provider = validateProvider(
    providerSigned.envelope.payload,
    providerSigned.producer,
  );
  if (
    !Array.isArray(evidence.probe_receipts)
    || evidence.probe_receipts.length !== D2_PROBE_KINDS.length
  ) {
    fail("every production probe requires one individually signed receipt");
  }
  const probes: D2ProductionProbe[] = [];
  const producerIds = new Set<string>([providerSigned.envelope.producer_id]);
  const statementDigests = [
    await sha256Hex(canonicalJson(providerSigned.envelope)),
  ];
  for (const [index, receipt] of evidence.probe_receipts.entries()) {
    const signed = await verifySigned<D2ProductionProbe>(
      receipt,
      registry,
      "production-probe",
      `probe receipt ${index}`,
    );
    const probe = objectValue(signed.envelope.payload, `probe payload ${index}`);
    exactKeys(probe, [
      "format",
      "environment",
      "probe_id",
      "kind",
      "observed_at_ms",
      "transcript_sha256",
      "binding",
      "outcome",
    ], `probe payload ${index}`);
    if (
      probe.format !== D2_PROBE_FORMAT
      || probe.environment !== "production"
      || typeof probe.probe_id !== "string"
      || !PROBE_ID_RE.test(probe.probe_id)
      || !D2_PROBE_KINDS.includes(probe.kind as D2ProbeKind)
      || typeof probe.transcript_sha256 !== "string"
      || !SHA256_RE.test(probe.transcript_sha256)
    ) {
      fail(`probe payload ${index} is synthetic, empty, or malformed`);
    }
    const probeObservedAt = positiveInt(
      probe.observed_at_ms,
      `probe ${index} observation time`,
    );
    if (probeObservedAt < provider.worker.activated_at_ms) {
      fail(`probe ${index} predates the authorized Worker activation`);
    }
    validateBinding(probe.binding, provider, signed.producer);
    validateProbeOutcome(probe.kind as D2ProbeKind, probe.outcome);
    if (
      probe.kind === "rollback-refusal"
      && objectValue(probe.outcome, "rollback-refusal outcome")
        .requested_target_version_id === provider.worker.version_id
    ) {
      fail("rollback refusal does not name an older deployment target");
    }
    probes.push(probe as unknown as D2ProductionProbe);
    producerIds.add(signed.envelope.producer_id);
    statementDigests.push(await sha256Hex(canonicalJson(signed.envelope)));
  }
  const kinds = probes.map((probe) => probe.kind).sort();
  const expectedKinds = [...D2_PROBE_KINDS].sort();
  if (
    kinds.length !== expectedKinds.length
    || kinds.some((kind, index) => kind !== expectedKinds[index])
    || new Set(probes.map((probe) => probe.probe_id)).size !== probes.length
    || new Set(probes.map((probe) => probe.transcript_sha256)).size
      !== probes.length
  ) {
    fail("production probe kinds, ids, or transcripts are missing or duplicated");
  }
  const cleanup = probes.find((probe) => probe.kind === "cleanup")!;
  const covered = (cleanup.outcome.covered_probe_ids as string[]).slice().sort();
  const witnessIds = probes
    .filter((probe) => !["cleanup", "rollback-refusal"].includes(probe.kind))
    .map((probe) => probe.probe_id)
    .sort();
  if (
    covered.length !== witnessIds.length
    || covered.some((id, index) => id !== witnessIds[index])
  ) {
    fail("cleanup receipt is not bound to every executed recovery probe");
  }
  return {
    provider,
    providerId: providerSigned.envelope.producer_id,
    probes,
    producerIds: [...producerIds].sort(),
    statementDigests,
  };
}

export async function verifyD2Migration0010ProductionRelease(
  input: unknown,
): Promise<D2ProductionReceipt> {
  const valid = await validateProduction(input, D2_TRUSTED_PRODUCERS);
  const witnessCounts = Object.fromEntries(
    D2_PROBE_KINDS.map((kind) => [kind, 1]),
  ) as Record<D2ProbeKind, 1>;
  return {
    format: D2_PRODUCTION_RECEIPT_FORMAT,
    verdict: "production-release-authorized",
    environment: "production",
    source: {
      commit_sha: D2_RELEASE_COMMIT,
      tree_sha: D2_RELEASE_TREE,
      manifest_sha256: D2_RELEASE_SOURCE_SHA256,
    },
    account_sha256: valid.provider.account_sha256,
    d1: {
      database_id: D2_DATABASE_ID,
      migration_observed_at_ms: valid.provider.d1.observed_at_ms,
    },
    worker: {
      version_id: valid.provider.worker.version_id,
      activated_at_ms: valid.provider.worker.activated_at_ms,
      traffic_percentage: 100,
    },
    r2: { bucket_name: D2_R2_BUCKET },
    cron: {
      natural_trigger_observation: "observed",
      scheduled_time_ms: valid.provider.cron.scheduled_time_ms,
      event_time_ms: valid.provider.cron.event_time_ms,
    },
    producer_ids: valid.producerIds,
    signed_statement_sha256: valid.statementDigests,
    witness_counts: witnessCounts,
    production_authorized: true,
  };
}

/**
 * Exercises the signature and semantic contract without emitting a production
 * receipt. Tests may inject ephemeral keys; production callers cannot.
 */
export async function verifyD2ProductionContractForTestsOnly(
  input: unknown,
  testRegistry: Readonly<Record<string, D2TrustedProducer>>,
): Promise<D2TestOnlyProductionValidation> {
  const valid = await validateProduction(input, testRegistry);
  return {
    format: "osl.cipher-store.d2-production-contract-test-result.v1",
    verdict: "test-only-signature-contract-valid",
    production_authorized: false,
    witness_kinds: valid.probes.map((probe) => probe.kind).sort(),
  };
}
