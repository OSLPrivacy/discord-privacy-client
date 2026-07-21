/// TTL sweep: deletes all blobs whose expires_at is in the past.
///
/// Runs every 5 minutes from the [triggers].crons in wrangler.toml.
/// Returns the aggregate row count to the in-process caller for tests and local
/// diagnostics. Production scheduling deliberately does not log the count:
/// even identifier-free counts reveal traffic volume and timing.

import type { Env } from "../env.js";
import { removeAttachmentStorage } from "../endpoints/attachment.js";
import {
  ATTACHMENT_SWEEP_BATCH_SIZE,
  MAX_LIVE_ATTACHMENT_ROWS,
} from "./attachment-limits.js";

export async function sweepExpired(env: Env): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  // D1 doesn't expose affected-rows directly; do a SELECT-count
  // first, then DELETE. The two queries don't have to be atomic --
  // a new expiry crossing the boundary mid-sweep just gets caught
  // on the next tick.
  const countRow = await env.DB.prepare(
    "SELECT COUNT(*) AS c FROM blobs WHERE expires_at < ?"
  )
    .bind(now)
    .first<{ c: number }>();
  const deleted = countRow?.c ?? 0;
  if (deleted > 0) {
    await env.DB.prepare("DELETE FROM blobs WHERE expires_at < ?")
      .bind(now)
      .run();
  }
  return deleted;
}

export async function sweepExpiredAttachments(env: Env): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  let deleted = 0;
  while (deleted < MAX_LIVE_ATTACHMENT_ROWS) {
    const result = await env.DB.prepare(
      `SELECT id, object_key, upload_id FROM attachment_objects
       WHERE expires_at < ? ORDER BY expires_at LIMIT ${ATTACHMENT_SWEEP_BATCH_SIZE}`,
    ).bind(now).all<{ id: string; object_key: string; upload_id: string | null }>();
    const rows = result.results ?? [];
    if (rows.length === 0) break;

    // R2 bulk deletion is bounded and completes before the corresponding D1
    // metadata is removed. A failure therefore leaves retryable metadata, not
    // an unindexed object. The quota ceiling means one cron invocation can
    // drain every row that was expired when it started.
    const completeKeys = rows
      .filter((row) => row.upload_id === null)
      .map((row) => row.object_key);
    if (completeKeys.length > 0) await env.ATTACHMENTS.delete(completeKeys);
    for (const row of rows) {
      if (row.upload_id) await removeAttachmentStorage(env, row);
    }
    const placeholders = rows.map(() => "?").join(", ");
    await env.DB.prepare(
      `DELETE FROM attachment_objects
       WHERE expires_at < ? AND id IN (${placeholders})`,
    ).bind(now, ...rows.map((row) => row.id)).run();
    deleted += rows.length;
    if (rows.length < ATTACHMENT_SWEEP_BATCH_SIZE) break;
  }
  return deleted;
}
