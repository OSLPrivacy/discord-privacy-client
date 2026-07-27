import { readFile } from "node:fs/promises";
import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import {
  ATTACHMENT_SWEEP_BATCH_SIZE,
  MAX_LIVE_ATTACHMENT_ROWS,
} from "../src/lib/attachment-limits.js";
import { sweepExpiredAttachments } from "../src/lib/sweep.js";
import { memoryR2, migratedD1 } from "./helpers/d1.js";

const DIGEST = "a".repeat(64);

function insertAttachment(
  db: ReturnType<typeof migratedD1>,
  row: {
    id: string;
    object_key: string;
    upload_id: string | null;
    expires_at?: number;
    state?: "uploading" | "completing" | "ready";
  },
): void {
  const now = Math.floor(Date.now() / 1000);
  db.exec(
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
    const migration = await readFile(
      new URL("../migrations/0004_attachment_capability_digests_and_quota.sql", import.meta.url),
      "utf8",
    );
    expect(migration).toContain("fetch_token_sha256_hex");
    expect(migration).not.toMatch(/\bfetch_token\s+TEXT\b/);
    expect(migration).not.toMatch(/CREATE\s+TRIGGER/i);
    expect(migration).not.toMatch(/^\s*(BEGIN|END)\s*;/im);
    const statements = migration.split(";").map((value) => value.trim()).filter(Boolean);
    expect(statements).toHaveLength(5);
    for (const statement of statements) {
      expect(statement).toMatch(/^(?:(?:--[^\n]*\n)|\s)*(?:DROP TABLE|CREATE TABLE|CREATE INDEX)/);
    }
  });

  it("drains a full-cap expired backlog in bounded bulk batches", async () => {
    const db = migratedD1();
    const r2 = memoryR2();
    for (let index = 0; index < MAX_LIVE_ATTACHMENT_ROWS; index++) {
      const objectKey = `attachments/${index}`;
      insertAttachment(db, {
        id: index.toString(16).padStart(32, "0"),
        object_key: objectKey,
        upload_id: null,
      });
      r2.objects.set(objectKey, new Uint8Array([index % 256]));
    }
    const remove = vi.spyOn(r2.bucket, "delete");
    const env = {
      DB: db.d1,
      ATTACHMENTS: r2.bucket,
    } as unknown as Env;

    await expect(sweepExpiredAttachments(env)).resolves.toBe(MAX_LIVE_ATTACHMENT_ROWS);
    expect(db.count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
    expect(r2.objects.size).toBe(0);
    expect(remove).toHaveBeenCalledTimes(Math.ceil(MAX_LIVE_ATTACHMENT_ROWS / ATTACHMENT_SWEEP_BATCH_SIZE));
  });

  it("keeps retryable metadata if an R2 batch deletion fails", async () => {
    const db = migratedD1();
    const r2 = memoryR2();
    insertAttachment(db, {
      id: "0".repeat(32),
      object_key: "attachments/0",
      upload_id: null,
    });
    r2.objects.set("attachments/0", new Uint8Array([1]));
    const remove = vi
      .spyOn(r2.bucket, "delete")
      .mockRejectedValueOnce(new Error("r2 unavailable"));
    const env = {
      DB: db.d1,
      ATTACHMENTS: r2.bucket,
    } as unknown as Env;

    await expect(sweepExpiredAttachments(env)).rejects.toThrow("r2 unavailable");
    expect(remove).toHaveBeenCalledOnce();
    expect(db.count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(1);
    expect(r2.objects.has("attachments/0")).toBe(true);
  });

  it("aborts an expired incomplete multipart upload before deleting its metadata", async () => {
    const db = migratedD1();
    const r2 = memoryR2();
    const objectKey = "attachments/incomplete";
    const upload = await r2.bucket.createMultipartUpload(objectKey);
    insertAttachment(db, {
      id: "1".repeat(32),
      object_key: objectKey,
      upload_id: upload.uploadId,
      state: "uploading",
    });
    const resume = vi.spyOn(r2.bucket, "resumeMultipartUpload");
    const env = {
      DB: db.d1,
      ATTACHMENTS: r2.bucket,
    } as unknown as Env;

    expect(r2.liveUploads()).toBe(1);
    await expect(sweepExpiredAttachments(env)).resolves.toBe(1);
    expect(resume).toHaveBeenCalledWith(objectKey, upload.uploadId);
    expect(r2.liveUploads()).toBe(0);
    expect(db.count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
  });
});
