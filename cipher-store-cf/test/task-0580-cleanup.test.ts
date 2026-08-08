import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  ABANDONED_UPLOAD_PART_HOURS,
  cleanupExpiredAndAbandonedAttachments,
} from "../src/lib/attachment-cleanup.js";
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

describe("TASK 0580 attachment cleanup job", () => {
  it("deleted=5 remaining=3 with expired_files=3 live_files=2 abandoned_parts=2 recent_part_minutes=5", async () => {
    const env = workerEnv();
    const now = Math.floor(Date.now() / 1000);
    const liveUntil = now + 24 * 60 * 60;

    // The delete-after date from 0579 has passed for these three completed
    // files. They have ordinary R2 bodies that must disappear with metadata.
    for (const suffix of ["1", "2", "3"]) {
      await insertAttachment(env, {
        id: suffix.repeat(32),
        objectKey: `attachments/task-0580-expired-${suffix}`,
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
        objectKey: `attachments/task-0580-live-${suffix}`,
        state: "ready",
        uploadId: null,
        expiresAt: liveUntil,
        contentExpiresAt: liveUntil,
        createdAt: now - 60,
      });
    }

    // Incomplete multipart work gets a one-hour inactivity deadline. These
    // two uploads started over that deadline ago and have a persisted part,
    // so cleanup must abort their R2 handles and remove their parent rows.
    for (const suffix of ["6", "7"]) {
      const objectKey = `attachments/task-0580-abandoned-${suffix}`;
      const upload = await env.ATTACHMENTS.createMultipartUpload(objectKey);
      await upload.uploadPart(1, new Uint8Array([Number(suffix)]));
      await insertAttachment(env, {
        id: suffix.repeat(32),
        objectKey,
        state: "uploading",
        uploadId: upload.uploadId,
        expiresAt: now - 1,
        contentExpiresAt: liveUntil,
        createdAt: now - (ABANDONED_UPLOAD_PART_HOURS * 60 * 60) - 1,
      });
      await d1Run(
        `INSERT INTO attachment_parts (attachment_id, part_number, size_bytes, etag)
         VALUES (?, 1, 1, NULL)`,
        suffix.repeat(32),
      );
    }

    // Five minutes of inactivity is inside the stated one-hour window.
    const freshKey = "attachments/task-0580-recent-part";
    const freshUpload = await env.ATTACHMENTS.createMultipartUpload(freshKey);
    await freshUpload.uploadPart(1, new Uint8Array([8]));
    await insertAttachment(env, {
      id: "8".repeat(32),
      objectKey: freshKey,
      state: "uploading",
      uploadId: freshUpload.uploadId,
      expiresAt: now + 24 * 60 * 60,
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
    expect(await env.ATTACHMENTS.head(freshKey)).toBeNull();
    await expect(freshUpload.uploadPart(2, new Uint8Array([9]))).resolves.toMatchObject({
      partNumber: 2,
    });
    console.log(
      "TASK0580 deleted=5 expired_files=3 abandoned_upload_parts=2 live_files=2 recent_part_minutes=5 remaining=3 abandoned_after_hours=1",
    );
  });
});
