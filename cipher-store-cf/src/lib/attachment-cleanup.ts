import type { Env } from "../env.js";

/** An incomplete multipart upload is abandoned after one hour. */
export const ABANDONED_UPLOAD_PART_SECONDS = 60 * 60;
export const ATTACHMENT_CLEANUP_BATCH_SIZE = 100;

interface CleanupCandidate {
  id: string;
  object_key: string;
  upload_id: string | null;
}

/**
 * Reclaim expired local attachment objects and abandoned multipart parts.
 *
 * Storage is removed before its D1 owner row. A failed R2 operation therefore
 * leaves both the encrypted copy and its metadata available for a later retry;
 * metadata deletion cascades any persisted part receipts only after the
 * multipart upload has been aborted.
 */
export async function cleanupExpiredAndAbandonedAttachments(
  env: Env,
  now = Math.floor(Date.now() / 1000),
): Promise<number> {
  const abandonedBefore = now - ABANDONED_UPLOAD_PART_SECONDS;
  const selected = await env.DB.prepare(
    `SELECT id, object_key, upload_id
       FROM attachment_objects
      WHERE (state = 'ready' AND expires_at <= ?)
         OR (state IN ('uploading', 'completing') AND created_at <= ?)
      ORDER BY created_at
      LIMIT ${ATTACHMENT_CLEANUP_BATCH_SIZE}`,
  ).bind(now, abandonedBefore).all<CleanupCandidate>();

  let removed = 0;
  for (const row of selected.results ?? []) {
    if (row.upload_id) {
      await env.ATTACHMENTS
        .resumeMultipartUpload(row.object_key, row.upload_id)
        .abort();
    }
    await env.ATTACHMENTS.delete(row.object_key);
    const deleted = await env.DB.prepare(
      "DELETE FROM attachment_objects WHERE id = ? AND object_key = ?",
    ).bind(row.id, row.object_key).run();
    // Cascaded child parts count in D1 changes; expose one reclaimed owner
    // per selected local attachment/upload instead.
    if ((deleted.meta.changes ?? 0) > 0) removed += 1;
  }
  return removed;
}
