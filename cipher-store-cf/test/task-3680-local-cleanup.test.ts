import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import { cleanupExpiredAndAbandonedAttachments } from "../src/lib/attachment-cleanup.js";
import { d1Count, d1Run, workerEnv } from "./helpers/workerd.js";

const DIGEST = "a".repeat(64);

async function insertAttachment(
  env: Env,
  row: {
    id: string;
    objectKey: string;
    state: "ready" | "uploading";
    uploadId: string | null;
    expiresAt: number;
    contentExpiresAt: number;
    createdAt: number;
  },
): Promise<void> {
  await d1Run(
    `INSERT INTO attachment_objects
       (id, object_key, size_bytes, expires_at, content_expires_at, created_at,
        fetch_token_sha256_hex, state, upload_id)
     VALUES (?, ?, 1, ?, ?, ?, ?, ?, ?)`,
    row.id,
    row.objectKey,
    row.expiresAt,
    row.contentExpiresAt,
    row.createdAt,
    DIGEST,
    row.state,
    row.uploadId,
  );
  if (row.state === "ready") {
    await env.ATTACHMENTS.put(row.objectKey, new Uint8Array([1]));
  }
}

describe("TASK 3680 local attachment cleanup", () => {
  it("removes exactly 5 expired copies or abandoned parts and leaves exactly 3 live items", async () => {
    const env = workerEnv();
    const now = Math.floor(Date.now() / 1000);
    const liveUntil = now + 24 * 60 * 60;

    // Expired ready rows represent local attachment copies. Their object and
    // metadata must go together; leaving either would retain recoverable data.
    for (const suffix of ["1", "2", "3"]) {
      await insertAttachment(env, {
        id: suffix.repeat(32),
        objectKey: `attachments/task-3680-expired-${suffix}`,
        state: "ready",
        uploadId: null,
        expiresAt: now - 1,
        contentExpiresAt: now - 1,
        createdAt: now - 60,
      });
    }

    for (const suffix of ["4", "5"]) {
      await insertAttachment(env, {
        id: suffix.repeat(32),
        objectKey: `attachments/task-3680-live-${suffix}`,
        state: "ready",
        uploadId: null,
        expiresAt: liveUntil,
        contentExpiresAt: liveUntil,
        createdAt: now - 60,
      });
    }

    // These two multipart uploads were abandoned. Persisted child part rows
    // must disappear with their parent, and abort prevents later publication.
    const abandonedUploads = [];
    for (const suffix of ["6", "7"]) {
      const objectKey = `attachments/task-3680-abandoned-${suffix}`;
      const upload = await env.ATTACHMENTS.createMultipartUpload(objectKey);
      await upload.uploadPart(1, new Uint8Array([Number(suffix)]));
      abandonedUploads.push(upload);
      await insertAttachment(env, {
        id: suffix.repeat(32),
        objectKey,
        state: "uploading",
        uploadId: upload.uploadId,
        expiresAt: now - 1,
        contentExpiresAt: liveUntil,
        createdAt: now - 60 * 60 - 1,
      });
      await d1Run(
        `INSERT INTO attachment_parts (attachment_id, part_number, size_bytes, etag)
         VALUES (?, 1, 1, NULL)`,
        suffix.repeat(32),
      );
    }

    const recentKey = "attachments/task-3680-recent-part";
    const recentUpload = await env.ATTACHMENTS.createMultipartUpload(recentKey);
    await recentUpload.uploadPart(1, new Uint8Array([8]));
    await insertAttachment(env, {
      id: "8".repeat(32),
      objectKey: recentKey,
      state: "uploading",
      uploadId: recentUpload.uploadId,
      expiresAt: liveUntil,
      contentExpiresAt: liveUntil,
      createdAt: now - 5 * 60,
    });
    await d1Run(
      `INSERT INTO attachment_parts (attachment_id, part_number, size_bytes, etag)
       VALUES (?, 1, 1, NULL)`,
      "8".repeat(32),
    );

    const result = await cleanupExpiredAndAbandonedAttachments(env, now);
    const remaining = await d1Count("SELECT COUNT(*) AS c FROM attachment_objects");

    expect(result).toBe(5);
    expect(remaining).toBe(3);
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_parts")).toBe(1);
    for (const suffix of ["1", "2", "3"]) {
      expect(await env.ATTACHMENTS.head(`attachments/task-3680-expired-${suffix}`)).toBeNull();
    }
    for (const suffix of ["4", "5"]) {
      expect(await env.ATTACHMENTS.head(`attachments/task-3680-live-${suffix}`)).not.toBeNull();
    }
    for (const upload of abandonedUploads) {
      await expect(upload.uploadPart(2, new Uint8Array([9]))).rejects.toThrow();
    }
    await expect(recentUpload.uploadPart(2, new Uint8Array([9]))).resolves.toMatchObject({
      partNumber: 2,
    });
    console.log(
      "TASK3680 removed=5 expired_local_files=3 live_files=2 abandoned_parts=2 recent_part_minutes=5 remaining=3",
    );
  });
});
