import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  D2_CRON,
  D2_CYCLE_MARKER,
  D2_DATABASE_ID,
  D2_DATABASE_NAME,
  D2_MIGRATION_0010_SHA256,
  D2_RECEIPT_FORMAT,
  D2_RECOVERY_MARKER,
  D2_RELEASE_COMMIT,
  D2_RELEASE_FORMAT,
  D2_RELEASE_SOURCE_FILES,
  D2_RELEASE_SOURCE_SHA256,
  D2_RELEASE_TREE,
  D2_REQUIRED_MIGRATIONS,
  D2_R2_BUCKET,
  D2_WORKER_NAME,
  type D2ReleaseProbeEvidence,
  verifyD2Migration0010Release,
} from "../scripts/d2-0010-release-contract.js";

const ACCOUNT_SHA256 = "a".repeat(64);
const VERSION_ID = "11111111-1111-4111-8111-111111111111";
const ROLLBACK_VERSION_ID = "22222222-2222-4222-8222-222222222222";
const PROJECT_ROOT = fileURLToPath(new URL("../", import.meta.url));

function evidence(): D2ReleaseProbeEvidence {
  return {
    format: D2_RELEASE_FORMAT,
    source: {
      commit_sha: D2_RELEASE_COMMIT,
      tree_sha: D2_RELEASE_TREE,
      manifest_sha256: D2_RELEASE_SOURCE_SHA256,
    },
    d1: {
      account_sha256: ACCOUNT_SHA256,
      database_id: D2_DATABASE_ID,
      database_name: D2_DATABASE_NAME,
      observed_at_ms: 1_000,
      applied_migrations: [...D2_REQUIRED_MIGRATIONS],
      migration_0010_sha256: D2_MIGRATION_0010_SHA256,
      recovery_marker: {
        format: D2_RECOVERY_MARKER,
        max_claims_per_cycle: 100,
      },
      claim_columns: ["claim_origin", "storage_fence_state"],
    },
    worker: {
      account_sha256: ACCOUNT_SHA256,
      worker_name: D2_WORKER_NAME,
      version_id: VERSION_ID,
      activated_at_ms: 2_000,
      traffic_percentage: 100,
      source: {
        commit_sha: D2_RELEASE_COMMIT,
        tree_sha: D2_RELEASE_TREE,
        manifest_sha256: D2_RELEASE_SOURCE_SHA256,
      },
      handlers: ["fetch", "scheduled"],
      d1_binding: {
        binding: "DB",
        database_id: D2_DATABASE_ID,
      },
      r2_binding: {
        binding: "ATTACHMENTS",
        bucket_name: D2_R2_BUCKET,
      },
    },
    r2: {
      account_sha256: ACCOUNT_SHA256,
      binding: "ATTACHMENTS",
      bucket_name: D2_R2_BUCKET,
    },
    scheduled: {
      worker_version_id: VERSION_ID,
      cron: D2_CRON,
      scheduled_time_ms: 3_000,
      event_time_ms: 3_010,
      outcome: "ok",
      marker: D2_CYCLE_MARKER,
      manual_invocation: false,
    },
    witnesses: {
      legacy_no_object: {
        count: 1,
        created_after_0009_cutoff: true,
        unlineaged: true,
        state_before: "completing",
        expected_size_bytes: 2,
        head_before: "absent",
        abort_outcome: "succeeded",
        post_abort_head: "absent",
        row_after: "removed",
        quota_after: "released",
      },
      exact_size: {
        count: 1,
        expected_size_bytes: 2,
        observed_size_bytes: 2,
        head_before: "exact",
        abort_calls: 0,
        delete_calls: 0,
        row_after: "ready",
        object_after: "retained",
        quota_after: "retained",
      },
      wrong_size: {
        count: 1,
        expected_size_bytes: 2,
        observed_size_bytes: 3,
        abort_order: 1,
        delete_order: 2,
        absence_cas_order: 3,
        post_delete_head: "absent",
        row_after: "removed",
        quota_after: "released",
      },
    },
    retry: {
      no_such_upload: {
        count: 1,
        discriminator: "code",
        value: "NoSuchUpload",
        post_abort_head: "absent",
        decision: "resume-idempotently",
      },
      unknown_abort: {
        count: 1,
        discriminator: "code",
        value: "InternalError",
        decision: "retain",
        delete_calls: 0,
        metadata_after: "retained",
        quota_after: "retained",
      },
      crash_retry: {
        count: 1,
        absence_fence_persisted: true,
        lease_version_increased: true,
        final_cleanup: "completed",
      },
    },
    cleanup: {
      created_rows: 3,
      created_completed_objects: 2,
      remaining_rows: 0,
      remaining_objects: 0,
      remaining_multipart_uploads: 0,
    },
    rollback: {
      requested_target_version_id: ROLLBACK_VERSION_ID,
      target_source_digest_sha256: "b".repeat(64),
      decision: "refused",
      reason: "migration-0010-before-worker",
      mutations_performed: 0,
    },
  };
}

function verify(value: unknown = evidence()) {
  return verifyD2Migration0010Release(value, {
    account_sha256: ACCOUNT_SHA256,
  });
}

describe("D2 migration-0010 release/probe contract", () => {
  it("accepts one exact nonsecret migration-before-Worker receipt", () => {
    const receipt = verify();
    expect(receipt).toMatchObject({
      format: D2_RECEIPT_FORMAT,
      verdict: "release-probe-accepted",
      source: {
        commit_sha: D2_RELEASE_COMMIT,
        tree_sha: D2_RELEASE_TREE,
        manifest_sha256: D2_RELEASE_SOURCE_SHA256,
      },
      worker: {
        version_id: VERSION_ID,
        traffic_percentage: 100,
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
        mutations_performed: 0,
      },
    });
    expect(JSON.stringify(receipt)).not.toMatch(
      /attachment_id|object_key|upload_id|fetch_token|ciphertext|plaintext|bearer/i,
    );
  });

  it("binds the reviewed source digest to the exact current Worker inputs", () => {
    const manifest = createHash("sha256");
    for (const relative of D2_RELEASE_SOURCE_FILES) {
      const bytes = readFileSync(
        fileURLToPath(new URL(`../${relative}`, import.meta.url)),
      );
      manifest.update(relative);
      manifest.update("\0");
      manifest.update(String(bytes.byteLength));
      manifest.update("\0");
      manifest.update(bytes);
    }
    expect(manifest.digest("hex")).toBe(D2_RELEASE_SOURCE_SHA256);

    const migration = readFileSync(
      fileURLToPath(
        new URL(
          "../migrations/0010_continuous_predecessor_recovery.sql",
          import.meta.url,
        ),
      ),
    );
    expect(createHash("sha256").update(migration).digest("hex")).toBe(
      D2_MIGRATION_0010_SHA256,
    );
    expect(PROJECT_ROOT.endsWith("cipher-store-cf/")).toBe(true);
  });

  it("refuses Worker-first and equal-time migration ordering", () => {
    for (const observedAt of [2_001, 2_000]) {
      const value = evidence();
      value.d1.observed_at_ms = observedAt;
      expect(() => verify(value)).toThrow(/fenced behind migration 0010/);
    }
  });

  it("refuses missing, stale, reordered, or malformed migration 0010 state", () => {
    const missing = evidence();
    missing.d1.applied_migrations.pop();
    expect(() => verify(missing)).toThrow(/migrations/);

    const stale = evidence();
    stale.d1.migration_0010_sha256 = "c".repeat(64);
    expect(() => verify(stale)).toThrow(/missing or stale/);

    const reordered = evidence();
    [reordered.d1.applied_migrations[8], reordered.d1.applied_migrations[9]] = [
      reordered.d1.applied_migrations[9]!,
      reordered.d1.applied_migrations[8]!,
    ];
    expect(() => verify(reordered)).toThrow(/reordered/);

    const malformed = evidence();
    malformed.d1.recovery_marker.max_claims_per_cycle = 99;
    expect(() => verify(malformed)).toThrow(/marker/);
  });

  it("refuses a wrong account, D1 database, or R2 bucket", () => {
    const wrongAccount = evidence();
    wrongAccount.worker.account_sha256 = "d".repeat(64);
    expect(() => verify(wrongAccount)).toThrow(/account/);

    const wrongDatabase = evidence();
    wrongDatabase.worker.d1_binding.database_id =
      "33333333-3333-4333-8333-333333333333";
    expect(() => verify(wrongDatabase)).toThrow(/D1 binding/);

    const wrongBucket = evidence();
    wrongBucket.r2.bucket_name = "lookalike-bucket";
    expect(() => verify(wrongBucket)).toThrow(/R2 account, binding, or bucket/);
  });

  it("refuses source/deployment/version and scheduled-trigger mismatches", () => {
    const source = evidence();
    source.worker.source.manifest_sha256 = "e".repeat(64);
    expect(() => verify(source)).toThrow(/exact reviewed/);

    const version = evidence();
    version.scheduled.worker_version_id = ROLLBACK_VERSION_ID;
    expect(() => verify(version)).toThrow(/scheduled trigger/);

    const cron = evidence();
    cron.scheduled.cron = "*/10 * * * *";
    expect(() => verify(cron)).toThrow(/scheduled trigger/);

    const manual = evidence();
    manual.scheduled.manual_invocation = true;
    expect(() => verify(manual)).toThrow(/manual/);
  });

  it("refuses every empty or non-load-bearing recovery witness", () => {
    const emptyLegacy = evidence();
    emptyLegacy.witnesses.legacy_no_object.count = 0;
    expect(() => verify(emptyLegacy)).toThrow(/legacy/);

    const emptyExact = evidence();
    emptyExact.witnesses.exact_size.count = 0;
    expect(() => verify(emptyExact)).toThrow(/exact-size/);

    const emptyWrong = evidence();
    emptyWrong.witnesses.wrong_size.count = 0;
    expect(() => verify(emptyWrong)).toThrow(/wrong-size/);

    const deletedExact = evidence();
    deletedExact.witnesses.exact_size.delete_calls = 1;
    expect(() => verify(deletedExact)).toThrow(/reachable ciphertext/);

    const wrongOrder = evidence();
    wrongOrder.witnesses.wrong_size.abort_order = 2;
    wrongOrder.witnesses.wrong_size.delete_order = 1;
    expect(() => verify(wrongOrder)).toThrow(/ordering/);
  });

  it("accepts only an exact code/name NoSuchUpload shape", () => {
    const messageOnly = evidence();
    messageOnly.retry.no_such_upload.discriminator = "message";
    expect(() => verify(messageOnly)).toThrow(/shape/);

    const widened = evidence();
    widened.retry.no_such_upload.value = "NoSuchUpload: probably gone";
    expect(() => verify(widened)).toThrow(/shape/);

    const accessDenied = evidence();
    accessDenied.retry.no_such_upload.value = "AccessDenied";
    expect(() => verify(accessDenied)).toThrow(/shape/);

    const byName = evidence();
    byName.retry.no_such_upload.discriminator = "name";
    expect(() => verify(byName)).not.toThrow();
  });

  it("retains ambiguity and refuses incomplete retry or probe cleanup", () => {
    const deleteUnknown = evidence();
    deleteUnknown.retry.unknown_abort.delete_calls = 1;
    expect(() => verify(deleteUnknown)).toThrow(/unknown abort/);

    const noFence = evidence();
    noFence.retry.crash_retry.absence_fence_persisted = false;
    expect(() => verify(noFence)).toThrow(/crash\/retry/);

    const remainingRow = evidence();
    remainingRow.cleanup.remaining_rows = 1;
    expect(() => verify(remainingRow)).toThrow(/cleanup/);

    const emptySetup = evidence();
    emptySetup.cleanup.created_rows = 0;
    expect(() => verify(emptySetup)).toThrow(/created probe rows/);
  });

  it("requires a mutation-free refusal of a genuinely older rollback target", () => {
    const allowed = evidence();
    allowed.rollback.decision = "allowed";
    expect(() => verify(allowed)).toThrow(/rollback/);

    const mutated = evidence();
    mutated.rollback.mutations_performed = 1;
    expect(() => verify(mutated)).toThrow(/rollback/);

    const currentTarget = evidence();
    currentTarget.rollback.requested_target_version_id = VERSION_ID;
    expect(() => verify(currentTarget)).toThrow(/rollback/);

    const sameSource = evidence();
    sameSource.rollback.target_source_digest_sha256 =
      D2_RELEASE_SOURCE_SHA256;
    expect(() => verify(sameSource)).toThrow(/rollback/);
  });

  it("refuses unexpected fields instead of silently retaining secrets", () => {
    const value = evidence() as D2ReleaseProbeEvidence & {
      attachment_id?: string;
    };
    value.attachment_id = "should-never-enter-a-receipt";
    expect(() => verify(value)).toThrow(/unexpected or missing fields/);
  });
});
