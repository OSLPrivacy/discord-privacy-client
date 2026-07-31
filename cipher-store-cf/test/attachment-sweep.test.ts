import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import migration from "../migrations/0004_attachment_capability_digests_and_quota.sql?raw";
import {
  ATTACHMENT_SWEEP_BATCH_SIZE,
  INCOMPLETE_SESSION_TTL_SECONDS,
  MAX_LIVE_ATTACHMENT_ROWS,
} from "../src/lib/attachment-limits.js";
import { sweepExpiredAttachments } from "../src/lib/sweep.js";
import {
  d1Count,
  d1Run,
  workerEnv,
} from "./helpers/workerd.js";

const DIGEST = "a".repeat(64);

function r2WithOverrides(real: R2Bucket, overrides: Record<PropertyKey, unknown>): R2Bucket {
  return new Proxy(real, {
    get(target, property, receiver) {
      if (property in overrides) return overrides[property];
      const value = Reflect.get(target, property, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

async function insertAttachment(
  row: {
    id: string;
    object_key: string;
    upload_id: string | null;
    expires_at?: number;
    content_expires_at?: number | null;
    created_at?: number;
    state?: "uploading" | "completing" | "ready";
  },
): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await d1Run(
    `INSERT INTO attachment_objects
       (id, object_key, size_bytes, expires_at, content_expires_at, created_at,
        fetch_token_sha256_hex, state, upload_id)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    row.id,
    row.object_key,
    1,
    row.expires_at ?? now - 1,
    row.content_expires_at === undefined
      ? row.expires_at ?? now - 1
      : row.content_expires_at,
    row.created_at ?? now - 60,
    DIGEST,
    row.state ?? "ready",
    row.upload_id,
  );
}

describe("attachment quota and expiry sweep", () => {
  it("uses Wrangler-splittable DDL for digest-only metadata", async () => {
    expect(migration).toContain("fetch_token_sha256_hex");
    expect(migration).not.toMatch(/\bfetch_token\s+TEXT\b/);
    expect(migration).not.toMatch(/CREATE\s+TRIGGER/i);
    expect(migration).not.toMatch(/^\s*(BEGIN|END)\s*;/im);
    const statements = migration.split(";").map((value: string) => value.trim()).filter(Boolean);
    expect(statements).toHaveLength(5);
    for (const statement of statements) {
      expect(statement).toMatch(/^(?:(?:--[^\n]*\n)|\s)*(?:DROP TABLE|CREATE TABLE|CREATE INDEX)/);
    }
  });

  it("drains a full-cap expired backlog in bounded bulk batches", async () => {
    const real = workerEnv();
    for (let index = 0; index < MAX_LIVE_ATTACHMENT_ROWS; index++) {
      const objectKey = `attachments/${index}`;
      await insertAttachment({
        id: index.toString(16).padStart(32, "0"),
        object_key: objectKey,
        upload_id: null,
      });
      await real.ATTACHMENTS.put(objectKey, new Uint8Array([index % 256]));
    }
    const remove = vi.fn((key: string | string[]) => real.ATTACHMENTS.delete(key));
    const env = {
      ...real,
      ATTACHMENTS: r2WithOverrides(real.ATTACHMENTS, { delete: remove }),
    } as unknown as Env;

    let completed = 0;
    while (completed < MAX_LIVE_ATTACHMENT_ROWS) {
      const result = await sweepExpiredAttachments(env);
      expect(result.failed).toBe(0);
      expect(result.completed).toBe(result.claimed);
      expect(result.claimed).toBeLessThanOrEqual(ATTACHMENT_SWEEP_BATCH_SIZE);
      completed += result.completed;
    }
    expect(completed).toBe(MAX_LIVE_ATTACHMENT_ROWS);
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
    expect(await real.ATTACHMENTS.head("attachments/0")).toBeNull();
    expect(await real.ATTACHMENTS.head(`attachments/${MAX_LIVE_ATTACHMENT_ROWS - 1}`)).toBeNull();
    expect(remove).toHaveBeenCalledTimes(MAX_LIVE_ATTACHMENT_ROWS);
    // Slow, not stuck. This is the only spec that fills the whole
    // MAX_LIVE_ATTACHMENT_ROWS quota cap and then drains it -- ~1000 D1 + R2
    // operations against miniflare where every other spec does a handful --
    // and its cost is superlinear in row count on a 2-core runner: the
    // 101-row sibling spec below is 1.2x slower on CI than locally, this one
    // is 4.9x. The CI run that reported "timed out in 40000ms" also reported
    // the body itself finishing at 59439ms, so the work completes; only the
    // budget was wrong.
    //
    // 90s, not more: the ceiling still has to be low enough that a real hang
    // fails the run promptly instead of quietly costing minutes, and low
    // enough that this spec cannot monopolise the worker pool and starve the
    // specs vitest runs alongside it. Seeding these rows concurrently was
    // tried and is *worse* -- it took the spec to 115890ms on CI and timed
    // out three unrelated files -- so the seeding loop stays sequential.
  }, 90_000);

  it("reclaims more than one legacy selection batch through isolated claims", async () => {
    const real = workerEnv();
    const rows = ATTACHMENT_SWEEP_BATCH_SIZE + 1;
    for (let index = 0; index < rows; index++) {
      const objectKey = `attachments/d1-boundary/${index}`;
      await insertAttachment({
        id: `d1${index.toString(16).padStart(30, "0")}`,
        object_key: objectKey,
        upload_id: null,
      });
      await real.ATTACHMENTS.put(objectKey, new Uint8Array([index % 256]));
    }

    await expect(sweepExpiredAttachments(real)).resolves.toEqual({
      claimed: ATTACHMENT_SWEEP_BATCH_SIZE,
      completed: ATTACHMENT_SWEEP_BATCH_SIZE,
      failed: 0,
    });
    await expect(sweepExpiredAttachments(real)).resolves.toEqual({
      claimed: 1,
      completed: 1,
      failed: 0,
    });
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
    expect(await real.ATTACHMENTS.head("attachments/d1-boundary/0")).toBeNull();
    expect(await real.ATTACHMENTS.head(`attachments/d1-boundary/${rows - 1}`)).toBeNull();
  });

  it("keeps retryable metadata if an R2 deletion fails", async () => {
    const real = workerEnv();
    await insertAttachment({
      id: "0".repeat(32),
      object_key: "attachments/0",
      upload_id: null,
    });
    await real.ATTACHMENTS.put("attachments/0", new Uint8Array([1]));
    const remove = vi.fn().mockRejectedValueOnce(new Error("r2 unavailable"));
    const env = {
      ...real,
      ATTACHMENTS: r2WithOverrides(real.ATTACHMENTS, { delete: remove }),
    } as unknown as Env;

    await expect(sweepExpiredAttachments(env)).resolves.toEqual({
      claimed: 1,
      completed: 0,
      failed: 1,
    });
    expect(remove).toHaveBeenCalledOnce();
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(1);
    expect(await real.ATTACHMENTS.head("attachments/0")).not.toBeNull();
  });

  it("aborts an expired incomplete multipart upload before deleting its metadata", async () => {
    const real = workerEnv();
    const objectKey = "attachments/incomplete";
    const upload = await real.ATTACHMENTS.createMultipartUpload(objectKey);
    await insertAttachment({
      id: "1".repeat(32),
      object_key: objectKey,
      upload_id: upload.uploadId,
      state: "uploading",
    });
    const resume = vi.fn((key: string, uploadId: string) =>
      real.ATTACHMENTS.resumeMultipartUpload(key, uploadId)
    );
    const env = {
      ...real,
      ATTACHMENTS: r2WithOverrides(real.ATTACHMENTS, { resumeMultipartUpload: resume }),
    } as unknown as Env;

    await expect(upload.uploadPart(1, new Uint8Array([1]))).resolves.toMatchObject({ partNumber: 1 });
    await expect(sweepExpiredAttachments(env)).resolves.toEqual({
      claimed: 1,
      completed: 1,
      failed: 0,
    });
    expect(resume).toHaveBeenCalledWith(objectKey, upload.uploadId);
    await expect(upload.uploadPart(2, new Uint8Array([2]))).rejects.toThrow();
    expect(await real.ATTACHMENTS.head(objectKey)).toBeNull();
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
  });

  it("expires a stale legacy no-part reservation and aborts R2 before metadata removal", async () => {
    const real = workerEnv();
    const now = Math.floor(Date.now() / 1000);
    const id = "2".repeat(32);
    const objectKey = "attachments/legacy-no-parts";
    const upload = await real.ATTACHMENTS.createMultipartUpload(objectKey);
    await insertAttachment({
      id,
      object_key: objectKey,
      upload_id: upload.uploadId,
      state: "uploading",
      // Old Worker rows can hold the caller's long content TTL directly in
      // expires_at while leaving migration 0006's promised-expiry column null.
      expires_at: now + 24 * 60 * 60,
      content_expires_at: null,
      created_at: now - INCOMPLETE_SESSION_TTL_SECONDS - 1,
    });

    const abortObservedWithMetadata = vi.fn();
    const resume = vi.fn((key: string, uploadId: string) => {
      const multipart = real.ATTACHMENTS.resumeMultipartUpload(key, uploadId);
      return new Proxy(multipart, {
        get(target, property, receiver) {
          if (property === "abort") {
            return async () => {
              abortObservedWithMetadata(
                await d1Count(
                  "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
                  id,
                ),
              );
              return target.abort();
            };
          }
          const value = Reflect.get(target, property, receiver);
          return typeof value === "function" ? value.bind(target) : value;
        },
      });
    });
    const env = {
      ...real,
      ATTACHMENTS: r2WithOverrides(real.ATTACHMENTS, {
        resumeMultipartUpload: resume,
      }),
    } as unknown as Env;

    await expect(upload.uploadPart(1, new Uint8Array([1]))).resolves.toMatchObject({
      partNumber: 1,
    });
    await expect(sweepExpiredAttachments(env)).resolves.toEqual({
      claimed: 1,
      completed: 1,
      failed: 0,
    });
    expect(resume).toHaveBeenCalledWith(objectKey, upload.uploadId);
    expect(abortObservedWithMetadata).toHaveBeenCalledWith(1);
    await expect(upload.uploadPart(2, new Uint8Array([2]))).rejects.toThrow();
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
        id,
      ),
    ).toBe(0);
  });

  it("retains marked legacy metadata when multipart abort fails", async () => {
    const real = workerEnv();
    const now = Math.floor(Date.now() / 1000);
    const id = "3".repeat(32);
    const objectKey = "attachments/legacy-abort-retry";
    const upload = await real.ATTACHMENTS.createMultipartUpload(objectKey);
    await insertAttachment({
      id,
      object_key: objectKey,
      upload_id: upload.uploadId,
      state: "uploading",
      expires_at: now + 24 * 60 * 60,
      content_expires_at: null,
      created_at: now - INCOMPLETE_SESSION_TTL_SECONDS - 1,
    });

    const abort = vi.fn(async () => {
      expect(
        await d1Count(
          "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
          id,
        ),
      ).toBe(1);
      throw new Error("r2 abort unavailable");
    });
    const resume = vi.fn((key: string, uploadId: string) => {
      const multipart = real.ATTACHMENTS.resumeMultipartUpload(key, uploadId);
      return new Proxy(multipart, {
        get(target, property, receiver) {
          if (property === "abort") return abort;
          const value = Reflect.get(target, property, receiver);
          return typeof value === "function" ? value.bind(target) : value;
        },
      });
    });
    const env = {
      ...real,
      ATTACHMENTS: r2WithOverrides(real.ATTACHMENTS, {
        resumeMultipartUpload: resume,
      }),
    } as unknown as Env;

    await expect(sweepExpiredAttachments(env)).resolves.toEqual({
      claimed: 1,
      completed: 0,
      failed: 1,
    });
    expect(abort).toHaveBeenCalledOnce();
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
        id,
      ),
    ).toBe(1);
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ? AND expires_at < ?",
        id,
        now,
      ),
    ).toBe(1);
    await expect(upload.uploadPart(1, new Uint8Array([1]))).resolves.toMatchObject({
      partNumber: 1,
    });
  });

  it("does not mark fresh, current-schema, or part-receipted reservations", async () => {
    const real = workerEnv();
    const now = Math.floor(Date.now() / 1000);
    const currentId = "4".repeat(32);
    const currentKey = "attachments/current-schema";
    const currentUpload = await real.ATTACHMENTS.createMultipartUpload(currentKey);
    await insertAttachment({
      id: currentId,
      object_key: currentKey,
      upload_id: currentUpload.uploadId,
      state: "uploading",
      expires_at: now + 24 * 60 * 60,
      content_expires_at: now + 24 * 60 * 60,
      created_at: now - INCOMPLETE_SESSION_TTL_SECONDS - 1,
    });

    const partId = "5".repeat(32);
    const partKey = "attachments/legacy-with-part";
    const partUpload = await real.ATTACHMENTS.createMultipartUpload(partKey);
    await insertAttachment({
      id: partId,
      object_key: partKey,
      upload_id: partUpload.uploadId,
      state: "uploading",
      expires_at: now + 24 * 60 * 60,
      content_expires_at: null,
      created_at: now - INCOMPLETE_SESSION_TTL_SECONDS - 1,
    });
    await d1Run(
      `INSERT INTO attachment_parts
         (attachment_id, part_number, size_bytes, etag)
       VALUES (?, 1, 1, NULL)`,
      partId,
    );

    const freshId = "6".repeat(32);
    const freshKey = "attachments/legacy-fresh";
    const freshUpload = await real.ATTACHMENTS.createMultipartUpload(freshKey);
    await insertAttachment({
      id: freshId,
      object_key: freshKey,
      upload_id: freshUpload.uploadId,
      state: "uploading",
      expires_at: now + 24 * 60 * 60,
      content_expires_at: null,
      created_at: now,
    });

    await expect(sweepExpiredAttachments(real)).resolves.toEqual({
      claimed: 0,
      completed: 0,
      failed: 0,
    });
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id IN (?, ?, ?)",
        currentId,
        partId,
        freshId,
      ),
    ).toBe(3);
    await expect(currentUpload.uploadPart(1, new Uint8Array([1]))).resolves.toMatchObject({
      partNumber: 1,
    });
    await expect(partUpload.uploadPart(1, new Uint8Array([1]))).resolves.toMatchObject({
      partNumber: 1,
    });
    await expect(freshUpload.uploadPart(1, new Uint8Array([1]))).resolves.toMatchObject({
      partNumber: 1,
    });
  });
});
