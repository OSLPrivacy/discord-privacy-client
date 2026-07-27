import { afterEach, describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import worker from "../src/index.js";
import {
  D2_CRON,
  D2_CYCLE_MARKER,
  D2_DATABASE_ID,
  D2_DATABASE_NAME,
  D2_MIGRATION_0010_SHA256,
  D2_RECOVERY_MARKER,
  D2_RELEASE_COMMIT,
  D2_RELEASE_FORMAT,
  D2_RELEASE_SOURCE_SHA256,
  D2_RELEASE_TREE,
  D2_REQUIRED_MIGRATIONS,
  D2_R2_BUCKET,
  D2_WORKER_NAME,
  type D2ReleaseProbeEvidence,
  verifyD2Migration0010Release,
} from "../scripts/d2-0010-release-contract.js";
import {
  d1All,
  d1Count,
  d1First,
  d1Run,
  workerEnv,
} from "./helpers/workerd.js";

const NOW = 1_900_000_000;
const ACCOUNT_SHA256 = "a".repeat(64);
const VERSION_ID = "11111111-1111-4111-8111-111111111111";
const ROLLBACK_VERSION_ID = "22222222-2222-4222-8222-222222222222";
const DIGEST = "d".repeat(64);

interface SeededCompleting {
  id: string;
  key: string;
  upload: R2MultipartUpload;
  part: R2UploadedPart;
}

async function seedCompleting(
  env: Env,
  id: string,
  key: string,
  expectedBytes: Uint8Array,
): Promise<SeededCompleting> {
  const upload = await env.ATTACHMENTS.createMultipartUpload(key);
  const part = await upload.uploadPart(1, expectedBytes);
  await d1Run(
    `INSERT INTO attachment_objects
       (id, object_key, size_bytes, expires_at, content_expires_at,
        created_at, fetch_token_sha256_hex, state, upload_id)
     VALUES (?, ?, ?, ?, ?, ?, ?, 'completing', ?)`,
    id,
    key,
    expectedBytes.byteLength,
    NOW - 1,
    NOW + 3600,
    NOW - 100,
    DIGEST,
    upload.uploadId,
  );
  await d1Run(
    `INSERT INTO attachment_parts
       (attachment_id, part_number, size_bytes, etag)
     VALUES (?, 1, ?, ?)`,
    id,
    expectedBytes.byteLength,
    part.etag,
  );
  return { id, key, upload, part };
}

function recordingEnv(
  real: Env,
  events: string[],
): Env {
  const bucket = new Proxy(real.ATTACHMENTS, {
    get(target, property, receiver) {
      if (property === "resumeMultipartUpload") {
        return (key: string, uploadId: string) => {
          const upload = target.resumeMultipartUpload(key, uploadId);
          return new Proxy(upload, {
            get(uploadTarget, uploadProperty, uploadReceiver) {
              if (uploadProperty === "abort") {
                return async () => {
                  events.push(`abort:${key}`);
                  await uploadTarget.abort();
                };
              }
              const value = Reflect.get(
                uploadTarget,
                uploadProperty,
                uploadReceiver,
              );
              return typeof value === "function"
                ? value.bind(uploadTarget)
                : value;
            },
          });
        };
      }
      if (property === "delete") {
        return async (key: string | string[]) => {
          const keys = Array.isArray(key) ? key : [key];
          for (const item of keys) events.push(`delete:${item}`);
          await target.delete(key);
        };
      }
      const value = Reflect.get(target, property, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as R2Bucket;

  const db = new Proxy(real.DB, {
    get(target, property, receiver) {
      if (property === "prepare") {
        return (sql: string) => {
          if (/SET storage_fence_state/.test(sql)) events.push("absence-cas");
          return target.prepare(sql);
        };
      }
      const value = Reflect.get(target, property, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as D1Database;
  return { ...real, DB: db, ATTACHMENTS: bucket };
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("local D2 migration-0010 release witness", () => {
  it("feeds nonempty post-cutoff recovery and cleanup outcomes into the exact receipt contract", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW * 1000);
    const real = workerEnv();
    await d1Run(
      `UPDATE attachment_predecessor_adoption
          SET migration_started_at = ?,
              eligible_created_through = ?
        WHERE singleton = 1`,
      NOW - 2_000,
      NOW - 1_000,
    );

    const legacy = await seedCompleting(
      real,
      "1".repeat(32),
      "attachments/d2-release-legacy-no-object",
      new Uint8Array([1, 1]),
    );
    const exact = await seedCompleting(
      real,
      "2".repeat(32),
      "attachments/d2-release-exact-size",
      new Uint8Array([2, 2]),
    );
    const wrong = await seedCompleting(
      real,
      "3".repeat(32),
      "attachments/d2-release-wrong-size",
      new Uint8Array([3, 3]),
    );
    await exact.upload.complete([
      { partNumber: 1, etag: exact.part.etag },
    ]);
    await real.ATTACHMENTS.put(
      wrong.key,
      new Uint8Array([3, 3, 3]),
    );

    expect(
      await d1Count(
        `SELECT COUNT(*) AS c
           FROM attachment_objects AS object_row
           JOIN attachment_predecessor_adoption AS adoption
             ON adoption.singleton = 1
          WHERE object_row.id IN (?, ?, ?)
            AND object_row.created_at > adoption.eligible_created_through
            AND NOT EXISTS (
              SELECT 1 FROM attachment_sweep_claims AS claim
               WHERE claim.attachment_id = object_row.id
            )`,
        legacy.id,
        exact.id,
        wrong.id,
      ),
    ).toBe(3);
    expect(await real.ATTACHMENTS.head(legacy.key)).toBeNull();
    expect(await real.ATTACHMENTS.head(exact.key)).toMatchObject({ size: 2 });
    expect(await real.ATTACHMENTS.head(wrong.key)).toMatchObject({ size: 3 });

    const events: string[] = [];
    const marker = vi.spyOn(console, "log").mockImplementation(() => undefined);
    await worker.scheduled(
      {
        cron: D2_CRON,
        scheduledTime: 3_000,
        type: "scheduled",
      } as ScheduledEvent,
      recordingEnv(real, events),
      {} as ExecutionContext,
    );

    expect(events).toEqual([
      `abort:${legacy.key}`,
      "absence-cas",
      `abort:${wrong.key}`,
      `delete:${wrong.key}`,
      "absence-cas",
    ]);
    expect(marker).toHaveBeenCalledWith(D2_CYCLE_MARKER);
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id IN (?, ?)",
        legacy.id,
        wrong.id,
      ),
    ).toBe(0);
    expect(
      await d1First<{ state: string; size_bytes: number }>(
        "SELECT state, size_bytes FROM attachment_objects WHERE id = ?",
        exact.id,
      ),
    ).toEqual({ state: "ready", size_bytes: 2 });
    expect(await real.ATTACHMENTS.head(exact.key)).toMatchObject({ size: 2 });
    expect(await real.ATTACHMENTS.head(wrong.key)).toBeNull();
    expect(await real.ATTACHMENTS.head(legacy.key)).toBeNull();

    await real.ATTACHMENTS.delete(exact.key);
    await d1Run("DELETE FROM attachment_objects WHERE id = ?", exact.id);
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id IN (?, ?, ?)",
        legacy.id,
        exact.id,
        wrong.id,
      ),
    ).toBe(0);
    expect(await real.ATTACHMENTS.head(exact.key)).toBeNull();
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_sweep_claims WHERE attachment_id IN (?, ?, ?)",
        legacy.id,
        exact.id,
        wrong.id,
      ),
    ).toBe(0);

    const applied = await d1All<{ name: string }>(
      "SELECT name FROM d1_migrations ORDER BY id",
    );
    const recovery = await d1First<{
      format: string;
      max_claims_per_cycle: number;
    }>(
      `SELECT format, max_claims_per_cycle
         FROM attachment_predecessor_recovery
        WHERE singleton = 1`,
    );
    const evidence: D2ReleaseProbeEvidence = {
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
        applied_migrations: applied.map((row) => row.name),
        migration_0010_sha256: D2_MIGRATION_0010_SHA256,
        recovery_marker: recovery,
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
        d1_binding: { binding: "DB", database_id: D2_DATABASE_ID },
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
    expect(
      verifyD2Migration0010Release(evidence, {
        account_sha256: ACCOUNT_SHA256,
      }),
    ).toMatchObject({
      verdict: "release-probe-accepted",
      witness_counts: {
        legacy_no_object: 1,
        exact_size: 1,
        wrong_size: 1,
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
  });
});
