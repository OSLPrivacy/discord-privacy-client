/**
 * Fail-closed release/probe contract for the D2 migration-0010 Worker.
 *
 * This module is deliberately pure. It does not invoke Wrangler, fetch,
 * migrate, deploy, schedule, roll back, or mutate D1/R2. An operator-side
 * collector may submit nonsecret observations, but this verifier emits a
 * receipt only when migration 0010 was observed before the exact Worker
 * version became active and every recovery/cleanup boundary is nonvacuous.
 */

export const D2_RELEASE_FORMAT =
  "osl.cipher-store.d2-migration-0010-release-evidence.v1";
export const D2_RECEIPT_FORMAT =
  "osl.cipher-store.d2-migration-0010-release-receipt.v1";
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

/**
 * Canonical source digest input, sorted bytewise by path. The digest is:
 * SHA-256(path + NUL + decimal-byte-length + NUL + bytes) for each entry.
 * Release-only scripts/tests are excluded because they are not Worker input.
 */
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

export interface D2ReleaseExpectation {
  /** SHA-256 of the production Cloudflare account id; the raw id is omitted. */
  account_sha256: string;
}

export interface D2ReleaseProbeEvidence {
  format: typeof D2_RELEASE_FORMAT;
  source: {
    commit_sha: string;
    tree_sha: string;
    manifest_sha256: string;
  };
  d1: {
    account_sha256: string;
    database_id: string;
    database_name: string;
    observed_at_ms: number;
    applied_migrations: string[];
    migration_0010_sha256: string;
    recovery_marker: {
      format: string;
      max_claims_per_cycle: number;
    };
    claim_columns: string[];
  };
  worker: {
    account_sha256: string;
    worker_name: string;
    version_id: string;
    activated_at_ms: number;
    traffic_percentage: number;
    source: {
      commit_sha: string;
      tree_sha: string;
      manifest_sha256: string;
    };
    handlers: string[];
    d1_binding: {
      binding: string;
      database_id: string;
    };
    r2_binding: {
      binding: string;
      bucket_name: string;
    };
  };
  r2: {
    account_sha256: string;
    binding: string;
    bucket_name: string;
  };
  scheduled: {
    worker_version_id: string;
    cron: string;
    scheduled_time_ms: number;
    event_time_ms: number;
    outcome: string;
    marker: string;
    manual_invocation: boolean;
  };
  witnesses: {
    legacy_no_object: {
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
    };
    exact_size: {
      count: number;
      expected_size_bytes: number;
      observed_size_bytes: number;
      head_before: string;
      abort_calls: number;
      delete_calls: number;
      row_after: string;
      object_after: string;
      quota_after: string;
    };
    wrong_size: {
      count: number;
      expected_size_bytes: number;
      observed_size_bytes: number;
      abort_order: number;
      delete_order: number;
      absence_cas_order: number;
      post_delete_head: string;
      row_after: string;
      quota_after: string;
    };
  };
  retry: {
    no_such_upload: {
      count: number;
      discriminator: string;
      value: string;
      post_abort_head: string;
      decision: string;
    };
    unknown_abort: {
      count: number;
      discriminator: string;
      value: string;
      decision: string;
      delete_calls: number;
      metadata_after: string;
      quota_after: string;
    };
    crash_retry: {
      count: number;
      absence_fence_persisted: boolean;
      lease_version_increased: boolean;
      final_cleanup: string;
    };
  };
  cleanup: {
    created_rows: number;
    created_completed_objects: number;
    remaining_rows: number;
    remaining_objects: number;
    remaining_multipart_uploads: number;
  };
  rollback: {
    requested_target_version_id: string;
    target_source_digest_sha256: string;
    decision: string;
    reason: string;
    mutations_performed: number;
  };
}

export interface D2ReleaseReceipt {
  format: typeof D2_RECEIPT_FORMAT;
  verdict: "release-probe-accepted";
  source: {
    commit_sha: typeof D2_RELEASE_COMMIT;
    tree_sha: typeof D2_RELEASE_TREE;
    manifest_sha256: typeof D2_RELEASE_SOURCE_SHA256;
  };
  account_sha256: string;
  d1: {
    database_id: typeof D2_DATABASE_ID;
    migration_0010_sha256: typeof D2_MIGRATION_0010_SHA256;
    migration_observed_at_ms: number;
  };
  worker: {
    worker_name: typeof D2_WORKER_NAME;
    version_id: string;
    activated_at_ms: number;
    traffic_percentage: 100;
  };
  r2: {
    binding: "ATTACHMENTS";
    bucket_name: typeof D2_R2_BUCKET;
  };
  scheduled: {
    cron: typeof D2_CRON;
    scheduled_time_ms: number;
    event_time_ms: number;
  };
  witness_counts: {
    legacy_no_object: 1;
    exact_size: 1;
    wrong_size: 1;
    no_such_upload_retry: 1;
    unknown_abort_retained: 1;
    crash_retry: 1;
  };
  cleanup: {
    remaining_rows: 0;
    remaining_objects: 0;
    remaining_multipart_uploads: 0;
  };
  rollback: {
    decision: "refused";
    reason: "migration-0010-before-worker";
    mutations_performed: 0;
  };
}

const SHA256_RE = /^[0-9a-f]{64}$/;
const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

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

function exactSource(value: unknown, label: string): void {
  const source = objectValue(value, label);
  exactKeys(
    source,
    ["commit_sha", "tree_sha", "manifest_sha256"],
    label,
  );
  if (
    source.commit_sha !== D2_RELEASE_COMMIT
    || source.tree_sha !== D2_RELEASE_TREE
    || source.manifest_sha256 !== D2_RELEASE_SOURCE_SHA256
  ) {
    fail(`${label} does not match the exact reviewed commit/tree/source digest`);
  }
}

function validateD1(
  value: unknown,
  expectation: D2ReleaseExpectation,
): number {
  const d1 = objectValue(value, "D1 evidence");
  exactKeys(d1, [
    "account_sha256",
    "database_id",
    "database_name",
    "observed_at_ms",
    "applied_migrations",
    "migration_0010_sha256",
    "recovery_marker",
    "claim_columns",
  ], "D1 evidence");
  if (
    d1.account_sha256 !== expectation.account_sha256
    || d1.database_id !== D2_DATABASE_ID
    || d1.database_name !== D2_DATABASE_NAME
  ) {
    fail("D1 account or database identity is wrong");
  }
  exactArray(
    d1.applied_migrations,
    D2_REQUIRED_MIGRATIONS,
    "D1 applied migrations",
  );
  if (d1.migration_0010_sha256 !== D2_MIGRATION_0010_SHA256) {
    fail("migration 0010 is missing or stale");
  }
  const marker = objectValue(d1.recovery_marker, "D1 recovery marker");
  exactKeys(marker, ["format", "max_claims_per_cycle"], "D1 recovery marker");
  if (
    marker.format !== D2_RECOVERY_MARKER
    || marker.max_claims_per_cycle !== 100
  ) {
    fail("migration 0010 recovery marker is missing or malformed");
  }
  exactArray(
    d1.claim_columns,
    ["claim_origin", "storage_fence_state"],
    "D1 claim columns",
  );
  return positiveInt(d1.observed_at_ms, "D1 observation time");
}

function validateWorker(
  value: unknown,
  expectation: D2ReleaseExpectation,
): { versionId: string; activatedAt: number } {
  const worker = objectValue(value, "Worker evidence");
  exactKeys(worker, [
    "account_sha256",
    "worker_name",
    "version_id",
    "activated_at_ms",
    "traffic_percentage",
    "source",
    "handlers",
    "d1_binding",
    "r2_binding",
  ], "Worker evidence");
  if (
    worker.account_sha256 !== expectation.account_sha256
    || worker.worker_name !== D2_WORKER_NAME
    || typeof worker.version_id !== "string"
    || !UUID_RE.test(worker.version_id)
    || worker.traffic_percentage !== 100
  ) {
    fail("Worker account, name, version, or traffic binding is invalid");
  }
  exactSource(worker.source, "Worker source");
  exactArray(worker.handlers, ["fetch", "scheduled"], "Worker handlers");

  const d1Binding = objectValue(worker.d1_binding, "Worker D1 binding");
  exactKeys(d1Binding, ["binding", "database_id"], "Worker D1 binding");
  if (
    d1Binding.binding !== "DB"
    || d1Binding.database_id !== D2_DATABASE_ID
  ) {
    fail("Worker D1 binding does not match the reviewed database");
  }
  const r2Binding = objectValue(worker.r2_binding, "Worker R2 binding");
  exactKeys(r2Binding, ["binding", "bucket_name"], "Worker R2 binding");
  if (
    r2Binding.binding !== "ATTACHMENTS"
    || r2Binding.bucket_name !== D2_R2_BUCKET
  ) {
    fail("Worker R2 binding does not match the reviewed bucket");
  }
  return {
    versionId: worker.version_id,
    activatedAt: positiveInt(worker.activated_at_ms, "Worker activation time"),
  };
}

function validateR2(
  value: unknown,
  expectation: D2ReleaseExpectation,
): void {
  const r2 = objectValue(value, "R2 evidence");
  exactKeys(r2, ["account_sha256", "binding", "bucket_name"], "R2 evidence");
  if (
    r2.account_sha256 !== expectation.account_sha256
    || r2.binding !== "ATTACHMENTS"
    || r2.bucket_name !== D2_R2_BUCKET
  ) {
    fail("R2 account, binding, or bucket is wrong");
  }
}

function validateScheduled(
  value: unknown,
  versionId: string,
  activatedAt: number,
): { scheduledAt: number; eventAt: number } {
  const scheduled = objectValue(value, "scheduled evidence");
  exactKeys(scheduled, [
    "worker_version_id",
    "cron",
    "scheduled_time_ms",
    "event_time_ms",
    "outcome",
    "marker",
    "manual_invocation",
  ], "scheduled evidence");
  const scheduledAt = positiveInt(
    scheduled.scheduled_time_ms,
    "scheduled time",
  );
  const eventAt = positiveInt(scheduled.event_time_ms, "scheduled event time");
  if (
    scheduled.worker_version_id !== versionId
    || scheduled.cron !== D2_CRON
    || scheduled.outcome !== "ok"
    || scheduled.marker !== D2_CYCLE_MARKER
    || scheduled.manual_invocation !== false
    || scheduledAt < activatedAt
    || eventAt < scheduledAt
  ) {
    fail("scheduled trigger is stale, manual, mismatched, or unsuccessful");
  }
  return { scheduledAt, eventAt };
}

function validateWitnesses(value: unknown): void {
  const witnesses = objectValue(value, "witness evidence");
  exactKeys(
    witnesses,
    ["legacy_no_object", "exact_size", "wrong_size"],
    "witness evidence",
  );

  const legacy = objectValue(
    witnesses.legacy_no_object,
    "legacy no-object witness",
  );
  exactKeys(legacy, [
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
    legacy.count !== 1
    || legacy.created_after_0009_cutoff !== true
    || legacy.unlineaged !== true
    || legacy.state_before !== "completing"
    || positiveInt(
      legacy.expected_size_bytes,
      "legacy expected size",
    ) <= 0
    || legacy.head_before !== "absent"
    || legacy.abort_outcome !== "succeeded"
    || legacy.post_abort_head !== "absent"
    || legacy.row_after !== "removed"
    || legacy.quota_after !== "released"
  ) {
    fail("legacy no-object witness is empty or does not prove safe recovery");
  }

  const exact = objectValue(witnesses.exact_size, "exact-size witness");
  exactKeys(exact, [
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
  const exactExpected = positiveInt(
    exact.expected_size_bytes,
    "exact witness expected size",
  );
  const exactObserved = positiveInt(
    exact.observed_size_bytes,
    "exact witness observed size",
  );
  if (
    exact.count !== 1
    || exactExpected !== exactObserved
    || exact.head_before !== "exact"
    || exact.abort_calls !== 0
    || exact.delete_calls !== 0
    || exact.row_after !== "ready"
    || exact.object_after !== "retained"
    || exact.quota_after !== "retained"
  ) {
    fail("exact-size witness is empty or reachable ciphertext was not retained");
  }

  const wrong = objectValue(witnesses.wrong_size, "wrong-size witness");
  exactKeys(wrong, [
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
  const wrongExpected = positiveInt(
    wrong.expected_size_bytes,
    "wrong-size expected size",
  );
  const wrongObserved = positiveInt(
    wrong.observed_size_bytes,
    "wrong-size observed size",
  );
  if (
    wrong.count !== 1
    || wrongExpected === wrongObserved
    || wrong.abort_order !== 1
    || wrong.delete_order !== 2
    || wrong.absence_cas_order !== 3
    || wrong.post_delete_head !== "absent"
    || wrong.row_after !== "removed"
    || wrong.quota_after !== "released"
  ) {
    fail("wrong-size witness is empty or cleanup ordering is unsafe");
  }
}

function validateRetry(value: unknown): void {
  const retry = objectValue(value, "retry evidence");
  exactKeys(
    retry,
    ["no_such_upload", "unknown_abort", "crash_retry"],
    "retry evidence",
  );
  const noSuch = objectValue(
    retry.no_such_upload,
    "NoSuchUpload retry witness",
  );
  exactKeys(noSuch, [
    "count",
    "discriminator",
    "value",
    "post_abort_head",
    "decision",
  ], "NoSuchUpload retry witness");
  if (
    noSuch.count !== 1
    || !["code", "name"].includes(String(noSuch.discriminator))
    || noSuch.value !== "NoSuchUpload"
    || noSuch.post_abort_head !== "absent"
    || noSuch.decision !== "resume-idempotently"
  ) {
    fail("NoSuchUpload retry shape is empty, widened, or unknown");
  }

  const unknown = objectValue(retry.unknown_abort, "unknown-abort witness");
  exactKeys(unknown, [
    "count",
    "discriminator",
    "value",
    "decision",
    "delete_calls",
    "metadata_after",
    "quota_after",
  ], "unknown-abort witness");
  if (
    unknown.count !== 1
    || !["code", "name"].includes(String(unknown.discriminator))
    || typeof unknown.value !== "string"
    || unknown.value.length === 0
    || unknown.value === "NoSuchUpload"
    || unknown.decision !== "retain"
    || unknown.delete_calls !== 0
    || unknown.metadata_after !== "retained"
    || unknown.quota_after !== "retained"
  ) {
    fail("unknown abort did not preserve metadata, quota, and storage");
  }

  const crash = objectValue(retry.crash_retry, "crash/retry witness");
  exactKeys(crash, [
    "count",
    "absence_fence_persisted",
    "lease_version_increased",
    "final_cleanup",
  ], "crash/retry witness");
  if (
    crash.count !== 1
    || crash.absence_fence_persisted !== true
    || crash.lease_version_increased !== true
    || crash.final_cleanup !== "completed"
  ) {
    fail("crash/retry witness is empty or not idempotently recoverable");
  }
}

function validateCleanup(value: unknown): void {
  const cleanup = objectValue(value, "probe cleanup");
  exactKeys(cleanup, [
    "created_rows",
    "created_completed_objects",
    "remaining_rows",
    "remaining_objects",
    "remaining_multipart_uploads",
  ], "probe cleanup");
  if (
    positiveInt(cleanup.created_rows, "created probe rows") !== 3
    || positiveInt(
      cleanup.created_completed_objects,
      "created completed probe objects",
    ) !== 2
    || nonnegativeInt(cleanup.remaining_rows, "remaining probe rows") !== 0
    || nonnegativeInt(
      cleanup.remaining_objects,
      "remaining probe objects",
    ) !== 0
    || nonnegativeInt(
      cleanup.remaining_multipart_uploads,
      "remaining multipart uploads",
    ) !== 0
  ) {
    fail("probe cleanup is empty, incomplete, or ambiguous");
  }
}

function validateRollback(value: unknown, workerVersionId: string): void {
  const rollback = objectValue(value, "rollback evidence");
  exactKeys(rollback, [
    "requested_target_version_id",
    "target_source_digest_sha256",
    "decision",
    "reason",
    "mutations_performed",
  ], "rollback evidence");
  if (
    typeof rollback.requested_target_version_id !== "string"
    || !UUID_RE.test(rollback.requested_target_version_id)
    || rollback.requested_target_version_id === workerVersionId
    || typeof rollback.target_source_digest_sha256 !== "string"
    || !SHA256_RE.test(rollback.target_source_digest_sha256)
    || rollback.target_source_digest_sha256 === D2_RELEASE_SOURCE_SHA256
    || rollback.decision !== "refused"
    || rollback.reason !== "migration-0010-before-worker"
    || rollback.mutations_performed !== 0
  ) {
    fail("rollback was not an exact, mutation-free refusal");
  }
}

export function verifyD2Migration0010Release(
  input: unknown,
  expectation: D2ReleaseExpectation,
): D2ReleaseReceipt {
  if (!SHA256_RE.test(expectation.account_sha256)) {
    fail("expected account fingerprint is absent or malformed");
  }
  const evidence = objectValue(input, "release evidence");
  exactKeys(evidence, [
    "format",
    "source",
    "d1",
    "worker",
    "r2",
    "scheduled",
    "witnesses",
    "retry",
    "cleanup",
    "rollback",
  ], "release evidence");
  if (evidence.format !== D2_RELEASE_FORMAT) {
    fail("release evidence format is unknown");
  }
  exactSource(evidence.source, "release source");
  const migrationObservedAt = validateD1(evidence.d1, expectation);
  const worker = validateWorker(evidence.worker, expectation);
  if (migrationObservedAt >= worker.activatedAt) {
    fail("Worker activation was not fenced behind migration 0010");
  }
  validateR2(evidence.r2, expectation);
  const scheduled = validateScheduled(
    evidence.scheduled,
    worker.versionId,
    worker.activatedAt,
  );
  validateWitnesses(evidence.witnesses);
  validateRetry(evidence.retry);
  validateCleanup(evidence.cleanup);
  validateRollback(evidence.rollback, worker.versionId);

  return {
    format: D2_RECEIPT_FORMAT,
    verdict: "release-probe-accepted",
    source: {
      commit_sha: D2_RELEASE_COMMIT,
      tree_sha: D2_RELEASE_TREE,
      manifest_sha256: D2_RELEASE_SOURCE_SHA256,
    },
    account_sha256: expectation.account_sha256,
    d1: {
      database_id: D2_DATABASE_ID,
      migration_0010_sha256: D2_MIGRATION_0010_SHA256,
      migration_observed_at_ms: migrationObservedAt,
    },
    worker: {
      worker_name: D2_WORKER_NAME,
      version_id: worker.versionId,
      activated_at_ms: worker.activatedAt,
      traffic_percentage: 100,
    },
    r2: {
      binding: "ATTACHMENTS",
      bucket_name: D2_R2_BUCKET,
    },
    scheduled: {
      cron: D2_CRON,
      scheduled_time_ms: scheduled.scheduledAt,
      event_time_ms: scheduled.eventAt,
    },
    witness_counts: {
      legacy_no_object: 1,
      exact_size: 1,
      wrong_size: 1,
      no_such_upload_retry: 1,
      unknown_abort_retained: 1,
      crash_retry: 1,
    },
    cleanup: {
      remaining_rows: 0,
      remaining_objects: 0,
      remaining_multipart_uploads: 0,
    },
    rollback: {
      decision: "refused",
      reason: "migration-0010-before-worker",
      mutations_performed: 0,
    },
  };
}
