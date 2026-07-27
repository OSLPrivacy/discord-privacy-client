import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import migration from "../migrations/0004_attachment_capability_digests_and_quota.sql?raw";
import {
  ATTACHMENT_SWEEP_BATCH_SIZE,
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
    row.expires_at ?? now - 1,
    now - 60,
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

    await expect(sweepExpiredAttachments(env)).resolves.toBe(MAX_LIVE_ATTACHMENT_ROWS);
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
    expect(await real.ATTACHMENTS.head("attachments/0")).toBeNull();
    expect(await real.ATTACHMENTS.head(`attachments/${MAX_LIVE_ATTACHMENT_ROWS - 1}`)).toBeNull();
    expect(remove).toHaveBeenCalledTimes(Math.ceil(MAX_LIVE_ATTACHMENT_ROWS / ATTACHMENT_SWEEP_BATCH_SIZE));
  }, 20_000);

  it("reclaims more than D1's 100-variable boundary in one sweep", async () => {
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

    await expect(sweepExpiredAttachments(real)).resolves.toBe(rows);
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
    expect(await real.ATTACHMENTS.head("attachments/d1-boundary/0")).toBeNull();
    expect(await real.ATTACHMENTS.head(`attachments/d1-boundary/${rows - 1}`)).toBeNull();
  });

  it("keeps retryable metadata if an R2 batch deletion fails", async () => {
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

    await expect(sweepExpiredAttachments(env)).rejects.toThrow("r2 unavailable");
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
    await expect(sweepExpiredAttachments(env)).resolves.toBe(1);
    expect(resume).toHaveBeenCalledWith(objectKey, upload.uploadId);
    await expect(upload.uploadPart(2, new Uint8Array([2]))).rejects.toThrow();
    expect(await real.ATTACHMENTS.head(objectKey)).toBeNull();
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
  });
});
