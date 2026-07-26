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

/// View-once link sweep. Two independent jobs, both unconditional:
///
///   1. **Destroy ciphertext** for every link whose reservation window
///      has closed or whose 1-hour TTL has passed. This is what makes
///      "the link dies after one view, or 60 seconds, whichever is
///      first" enforceable: it happens whether or not the recipient's
///      browser ever confirmed, and whether or not it is still running.
///   2. **Purge receipts** a day after expiry. A receipt is a row with
///      `data IS NULL` -- no ciphertext, no identifiers, only
///      `retrieved_at` at second granularity and a count -- kept only so
///      the sender can be told "Retrieved at HH:MM" or "Expired without
///      being retrieved".
///
/// Returns the aggregate count for tests; production never logs it.
export async function sweepExpiredLinks(env: Env): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  const burned = await env.DB.prepare(
    `UPDATE view_once_links
        SET data = NULL, size_bytes = 0, reserved_until = NULL, burned_at = ?
      WHERE data IS NOT NULL
        AND (expires_at < ? OR (reserved_until IS NOT NULL AND reserved_until < ?))`,
  )
    .bind(now, now, now)
    .run();
  await env.DB.prepare(
    "DELETE FROM view_once_links WHERE data IS NULL AND expires_at < ?",
  )
    .bind(now - LINK_RECEIPT_RETENTION_SECONDS)
    .run();
  return burned.meta?.changes ?? 0;
}

/// Mirror of `RECEIPT_RETENTION_SECONDS` in `endpoints/link.ts`, kept
/// here to avoid a cycle between the sweep and the endpoint module.
const LINK_RECEIPT_RETENTION_SECONDS = 24 * 60 * 60;

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
