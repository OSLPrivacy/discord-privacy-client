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
  INCOMPLETE_SESSION_TTL_SECONDS,
  MAX_LIVE_ATTACHMENT_ROWS,
} from "./attachment-limits.js";
import { MAX_LIVE_BLOB_ROWS } from "./blob-limits.js";

/// D1 caps a query at 100 bound parameters. Measured against real D1, not
/// recalled: 100 succeeds, 101 fails with `D1_ERROR: too many SQL variables`.
///
/// This is a PLATFORM limit, not a test artifact. `node:sqlite` allows 999, so
/// the previous shim-backed tests could never have caught it — and the delete
/// below used to bind one parameter per row plus `expires_at`, i.e. 101 at a
/// full batch of `ATTACHMENT_SWEEP_BATCH_SIZE`. The sweep therefore worked
/// under 100 expired rows and failed *entirely* at 100 or more, swallowed by
/// the `try/catch` in index.ts, so expired attachments were never reclaimed and
/// the quota filled permanently with no attacker involved.
///
/// 90 leaves headroom for the `expires_at` bind and any future predicate.
export const ATTACHMENT_D1_DELETE_CHUNK_IDS = 90;
export const BLOB_SWEEP_BATCH_SIZE = 100;
export const LINK_GRANT_SWEEP_BATCH_SIZE = 100;
export const LINK_GRANT_SWEEP_MAX_ROWS = 1000;

export async function sweepExpired(env: Env): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  let deleted = 0;
  while (deleted < MAX_LIVE_BLOB_ROWS) {
    const result = await env.DB.prepare(
      `SELECT id FROM blobs
       WHERE expires_at < ? ORDER BY expires_at LIMIT ${BLOB_SWEEP_BATCH_SIZE}`,
    ).bind(now).all<{ id: ArrayBuffer | Uint8Array }>();
    const rows = result.results ?? [];
    if (rows.length === 0) break;

    const ids = rows.map((row) => row.id);
    for (let offset = 0; offset < ids.length; offset += ATTACHMENT_D1_DELETE_CHUNK_IDS) {
      const chunk = ids.slice(offset, offset + ATTACHMENT_D1_DELETE_CHUNK_IDS);
      const placeholders = chunk.map(() => "?").join(", ");
      await env.DB.prepare(
        `DELETE FROM blobs
         WHERE expires_at < ? AND id IN (${placeholders})`,
      ).bind(now, ...chunk).run();
    }
    deleted += rows.length;
    if (rows.length < BLOB_SWEEP_BATCH_SIZE) break;
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
  const staleLegacyCreatedBefore = now - INCOMPLETE_SESSION_TTL_SECONDS;
  let deleted = 0;
  while (deleted < MAX_LIVE_ATTACHMENT_ROWS) {
    // Migration 0006 added `content_expires_at`, but a Worker that predates the
    // new session-budget write path can still create an `uploading` row with a
    // null promised expiry and the caller's long content TTL in `expires_at`.
    // With no accepted part receipt, that row is an abandoned quota reservation
    // once it is older than the same bounded hold applied to current sessions.
    //
    // Mark at most one sweep batch expired in a single atomic D1 statement.
    // This is metadata-only: storage removal still goes through the existing
    // R2-first path below, and any R2 failure leaves this now-expired row in D1
    // for a later retry.
    await env.DB.prepare(
      `UPDATE attachment_objects SET expires_at = ?
        WHERE id IN (
          SELECT candidate.id
            FROM attachment_objects AS candidate
           WHERE candidate.state = 'uploading'
             AND candidate.content_expires_at IS NULL
             AND candidate.created_at < ?
             AND candidate.expires_at >= ?
             AND NOT EXISTS (
               SELECT 1 FROM attachment_parts AS part
                WHERE part.attachment_id = candidate.id
             )
           ORDER BY candidate.created_at
           LIMIT ${ATTACHMENT_SWEEP_BATCH_SIZE}
        )`,
    ).bind(now - 1, staleLegacyCreatedBefore, now).run();

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
    const ids = rows.map((row) => row.id);
    for (let offset = 0; offset < ids.length; offset += ATTACHMENT_D1_DELETE_CHUNK_IDS) {
      const chunk = ids.slice(offset, offset + ATTACHMENT_D1_DELETE_CHUNK_IDS);
      const placeholders = chunk.map(() => "?").join(", ");
      await env.DB.prepare(
        `DELETE FROM attachment_objects
         WHERE expires_at < ? AND id IN (${placeholders})`,
      ).bind(now, ...chunk).run();
    }
    deleted += rows.length;
    if (rows.length < ATTACHMENT_SWEEP_BATCH_SIZE) break;
  }
  return deleted;
}

export async function sweepExpiredLinkGrantConsumptions(env: Env): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  let deleted = 0;
  while (deleted < LINK_GRANT_SWEEP_MAX_ROWS) {
    const result = await env.DB.prepare(
      `SELECT jti FROM link_grant_consumed
       WHERE expires_at < ? ORDER BY expires_at LIMIT ${LINK_GRANT_SWEEP_BATCH_SIZE}`,
    ).bind(now).all<{ jti: string }>();
    const rows = result.results ?? [];
    if (rows.length === 0) break;

    const ids = rows.map((row) => row.jti);
    for (let offset = 0; offset < ids.length; offset += ATTACHMENT_D1_DELETE_CHUNK_IDS) {
      const chunk = ids.slice(offset, offset + ATTACHMENT_D1_DELETE_CHUNK_IDS);
      const placeholders = chunk.map(() => "?").join(", ");
      await env.DB.prepare(
        `DELETE FROM link_grant_consumed
         WHERE expires_at < ? AND jti IN (${placeholders})`,
      ).bind(now, ...chunk).run();
    }
    deleted += rows.length;
    if (rows.length < LINK_GRANT_SWEEP_BATCH_SIZE) break;
  }
  return deleted;
}
