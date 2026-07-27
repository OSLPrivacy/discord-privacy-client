import type { Env } from "../env.js";
import { ATTACHMENT_SWEEP_BATCH_SIZE } from "./attachment-limits.js";

export const ATTACHMENT_SWEEP_LEASE_SECONDS = 2 * 60;
export const ATTACHMENT_SWEEP_RETRY_BASE_SECONDS = 5 * 60;
export const ATTACHMENT_SWEEP_RETRY_MAX_SECONDS = 6 * 60 * 60;

const WORKER_ID_RE = /^[0-9a-f]{32}$/;
const CLAIM_TOKEN_RE = /^[0-9a-f]{64}$/;
const PREDECESSOR_ADOPTION_FORMAT =
  "osl.cipher-store.predecessor-adoption.v1";

export type AttachmentClaimOrigin =
  | "lineaged"
  | "sweep"
  | "completion"
  | "predecessor_adoption";
export type AttachmentStorageFenceState =
  | "pending"
  | "object_absent_confirmed";

export interface AttachmentSweepClaim {
  attachment_id: string;
  object_key: string;
  size_bytes: number;
  content_expires_at: number | null;
  state: "uploading" | "completing" | "ready";
  upload_id: string | null;
  worker_id: string;
  claim_token: string;
  lease_version: number;
  lease_expires_at: number;
  attempt_count: number;
  claim_origin: AttachmentClaimOrigin;
  storage_fence_state: AttachmentStorageFenceState;
}

interface ClaimedIdentity {
  attachment_id: string;
  worker_id: string;
  claim_token: string;
  lease_version: number;
  lease_expires_at: number;
  attempt_count: number;
  claim_origin: AttachmentClaimOrigin;
  storage_fence_state: AttachmentStorageFenceState;
}

export type AttachmentSweepCompletion =
  | "completed"
  | "already_completed"
  | "stale";

export type AttachmentReadyCompletion = "ready" | "stale";
export type AttachmentStorageFenceCompletion =
  | "confirmed"
  | "already_confirmed"
  | "stale";

function randomHex(bytes: number): string {
  const value = new Uint8Array(bytes);
  crypto.getRandomValues(value);
  let output = "";
  for (const byte of value) output += byte.toString(16).padStart(2, "0");
  return output;
}

export function newAttachmentSweepWorkerId(): string {
  return randomHex(16);
}

/// Read-only migration-order gate. It names every claim column the Worker
/// relies on so an absent or partial 0009 schema refuses before legacy expiry
/// marking, R2 cleanup, or attachment metadata deletion.
export async function requireAttachmentSweepClaimSchema(env: Env): Promise<void> {
  await env.DB.prepare(
    `SELECT attachment_id, worker_id, claim_token, lease_version,
            lease_expires_at, retry_not_before, attempt_count, last_claimed_at,
            claim_origin, storage_fence_state
       FROM attachment_sweep_claims
      LIMIT 0`,
  ).all();
  const marker = await env.DB.prepare(
    `SELECT format, migration_started_at, eligible_created_through,
            max_claims_per_cycle
       FROM attachment_predecessor_adoption
      WHERE singleton = 1
      LIMIT 1`,
  ).first<{
    format: string;
    migration_started_at: number;
    eligible_created_through: number;
    max_claims_per_cycle: number;
  }>();
  if (
    marker?.format !== PREDECESSOR_ADOPTION_FORMAT
    || !Number.isSafeInteger(marker.migration_started_at)
    || marker.migration_started_at <= 0
    || !Number.isSafeInteger(marker.eligible_created_through)
    || marker.eligible_created_through <= marker.migration_started_at
    || marker.max_claims_per_cycle !== ATTACHMENT_SWEEP_BATCH_SIZE
  ) {
    throw new Error("attachment predecessor adoption marker is invalid");
  }
}

function validateWorkerId(workerId: string): void {
  if (!WORKER_ID_RE.test(workerId)) {
    throw new Error("attachment sweep Worker identity is invalid");
  }
}

function validateClaim(claim: AttachmentSweepClaim): void {
  if (
    !WORKER_ID_RE.test(claim.worker_id)
    || !CLAIM_TOKEN_RE.test(claim.claim_token)
    || !Number.isSafeInteger(claim.lease_version)
    || claim.lease_version <= 0
    || !["lineaged", "sweep", "completion", "predecessor_adoption"].includes(
      claim.claim_origin,
    )
    || !["pending", "object_absent_confirmed"].includes(
      claim.storage_fence_state,
    )
  ) {
    throw new Error("attachment sweep claim identity is invalid");
  }
}

/// Atomically claim one eligible expired attachment.
///
/// This is one SQLite write statement. Concurrent Worker invocations cannot
/// both obtain the same `(worker_id, claim_token, lease_version)` tuple. An
/// expired lease is recovered by increasing `lease_version`; an active or
/// backoff-delayed row is excluded. The follow-up read only materializes the
/// object coordinates for the exact claim already committed.
export async function claimNextExpiredAttachment(
  env: Env,
  workerId: string,
  now: number,
): Promise<AttachmentSweepClaim | null> {
  validateWorkerId(workerId);
  if (!Number.isSafeInteger(now) || now <= 0) {
    throw new Error("attachment sweep claim time is invalid");
  }
  const claimToken = randomHex(32);
  const leaseExpiresAt = now + ATTACHMENT_SWEEP_LEASE_SECONDS;
  const claimed = await env.DB.prepare(
    `INSERT INTO attachment_sweep_claims
       (attachment_id, worker_id, claim_token, lease_version,
        lease_expires_at, retry_not_before, attempt_count, last_claimed_at,
        claim_origin, storage_fence_state)
     SELECT candidate.id, ?, ?, 1, ?, 0, 1, ?,
            CASE
              WHEN candidate.state = 'completing'
               AND existing.attachment_id IS NULL
                THEN 'predecessor_adoption'
              ELSE 'sweep'
            END,
            'pending'
       FROM attachment_objects AS candidate
       LEFT JOIN attachment_sweep_claims AS existing
         ON existing.attachment_id = candidate.id
       LEFT JOIN attachment_predecessor_adoption AS adoption
         ON adoption.singleton = 1
      WHERE candidate.expires_at <= ?
        AND candidate.state IN ('uploading', 'completing', 'ready')
        AND (
          candidate.state <> 'completing'
          OR existing.attachment_id IS NOT NULL
          OR (
            adoption.format = 'osl.cipher-store.predecessor-adoption.v1'
            AND adoption.max_claims_per_cycle = 100
            AND candidate.created_at <= adoption.eligible_created_through
          )
        )
        AND (
          existing.attachment_id IS NULL
          OR (
            existing.lease_expires_at <= ?
            AND existing.retry_not_before <= ?
          )
        )
      ORDER BY candidate.expires_at, candidate.id
      LIMIT 1
     ON CONFLICT(attachment_id) DO UPDATE SET
       worker_id = excluded.worker_id,
       claim_token = excluded.claim_token,
       lease_version = attachment_sweep_claims.lease_version + 1,
       lease_expires_at = excluded.lease_expires_at,
       retry_not_before = 0,
       attempt_count = attachment_sweep_claims.attempt_count + 1,
       last_claimed_at = excluded.last_claimed_at
     WHERE attachment_sweep_claims.lease_expires_at <= ?
       AND attachment_sweep_claims.retry_not_before <= ?
     RETURNING attachment_id, worker_id, claim_token, lease_version,
               lease_expires_at, attempt_count, claim_origin,
               storage_fence_state`,
  ).bind(
    workerId,
    claimToken,
    leaseExpiresAt,
    now,
    now,
    now,
    now,
    now,
    now,
  ).first<ClaimedIdentity>();
  if (!claimed) return null;

  const row = await env.DB.prepare(
    `SELECT object_row.id AS attachment_id,
            object_row.object_key,
            object_row.size_bytes,
            object_row.content_expires_at,
            object_row.state,
            object_row.upload_id,
            claim.worker_id,
            claim.claim_token,
            claim.lease_version,
            claim.lease_expires_at,
            claim.attempt_count,
            claim.claim_origin,
            claim.storage_fence_state
       FROM attachment_objects AS object_row
       JOIN attachment_sweep_claims AS claim
         ON claim.attachment_id = object_row.id
      WHERE object_row.id = ?
        AND claim.worker_id = ?
        AND claim.claim_token = ?
        AND claim.lease_version = ?
      LIMIT 1`,
  ).bind(
    claimed.attachment_id,
    claimed.worker_id,
    claimed.claim_token,
    claimed.lease_version,
  ).first<AttachmentSweepClaim>();
  return row ?? null;
}

/// Acquire the same exclusive, versioned fence used by the sweeper before
/// multipart completion touches R2.
///
/// The claim write is one atomic SQLite statement. An active sweep claim
/// excludes completion; an expired claim can be recovered monotonically only
/// while the object is still an unexpired `uploading` session. The subsequent
/// state CAS is safe as a separate statement because the newly active claim
/// already excludes every sweep Worker.
export async function acquireAttachmentCompletionClaim(
  env: Env,
  attachmentId: string,
  workerId: string,
  now: number,
): Promise<AttachmentSweepClaim | null> {
  validateWorkerId(workerId);
  if (!WORKER_ID_RE.test(attachmentId) || !Number.isSafeInteger(now) || now <= 0) {
    throw new Error("attachment completion claim input is invalid");
  }
  const claimToken = randomHex(32);
  const leaseExpiresAt = now + ATTACHMENT_SWEEP_LEASE_SECONDS;
  const claimed = await env.DB.prepare(
    `INSERT INTO attachment_sweep_claims
       (attachment_id, worker_id, claim_token, lease_version,
        lease_expires_at, retry_not_before, attempt_count, last_claimed_at,
        claim_origin, storage_fence_state)
     SELECT candidate.id, ?, ?, 1, ?, 0, 1, ?, 'completion', 'pending'
       FROM attachment_objects AS candidate
       LEFT JOIN attachment_sweep_claims AS existing
         ON existing.attachment_id = candidate.id
      WHERE candidate.id = ?
        AND candidate.state = 'uploading'
        AND candidate.expires_at > ?
        AND (
          existing.attachment_id IS NULL
          OR (
            existing.lease_expires_at <= ?
            AND existing.retry_not_before <= ?
          )
        )
     ON CONFLICT(attachment_id) DO UPDATE SET
       worker_id = excluded.worker_id,
       claim_token = excluded.claim_token,
       lease_version = attachment_sweep_claims.lease_version + 1,
       lease_expires_at = excluded.lease_expires_at,
       retry_not_before = 0,
       attempt_count = attachment_sweep_claims.attempt_count + 1,
       last_claimed_at = excluded.last_claimed_at
     WHERE attachment_sweep_claims.lease_expires_at <= ?
       AND attachment_sweep_claims.retry_not_before <= ?
     RETURNING attachment_id, worker_id, claim_token, lease_version,
               lease_expires_at, attempt_count, claim_origin,
               storage_fence_state`,
  ).bind(
    workerId,
    claimToken,
    leaseExpiresAt,
    now,
    attachmentId,
    now,
    now,
    now,
    now,
    now,
  ).first<ClaimedIdentity>();
  if (!claimed) return null;

  const transitioned = await env.DB.prepare(
    `UPDATE attachment_objects
        SET state = 'completing'
      WHERE id = ?
        AND state = 'uploading'
        AND expires_at > ?
        AND EXISTS (
          SELECT 1 FROM attachment_sweep_claims AS owned
           WHERE owned.attachment_id = attachment_objects.id
             AND owned.worker_id = ?
             AND owned.claim_token = ?
             AND owned.lease_version = ?
             AND owned.lease_expires_at > ?
        )`,
  ).bind(
    attachmentId,
    now,
    claimed.worker_id,
    claimed.claim_token,
    claimed.lease_version,
    now,
  ).run();
  if ((transitioned.meta.changes ?? 0) !== 1) {
    await env.DB.prepare(
      `DELETE FROM attachment_sweep_claims
        WHERE attachment_id = ?
          AND worker_id = ?
          AND claim_token = ?
          AND lease_version = ?`,
    ).bind(
      attachmentId,
      claimed.worker_id,
      claimed.claim_token,
      claimed.lease_version,
    ).run();
    return null;
  }

  return env.DB.prepare(
    `SELECT object_row.id AS attachment_id,
            object_row.object_key,
            object_row.size_bytes,
            object_row.content_expires_at,
            object_row.state,
            object_row.upload_id,
            claim.worker_id,
            claim.claim_token,
            claim.lease_version,
            claim.lease_expires_at,
            claim.attempt_count,
            claim.claim_origin,
            claim.storage_fence_state
       FROM attachment_objects AS object_row
       JOIN attachment_sweep_claims AS claim
         ON claim.attachment_id = object_row.id
      WHERE object_row.id = ?
        AND object_row.state = 'completing'
        AND claim.worker_id = ?
        AND claim.claim_token = ?
        AND claim.lease_version = ?
      LIMIT 1`,
  ).bind(
    attachmentId,
    claimed.worker_id,
    claimed.claim_token,
    claimed.lease_version,
  ).first<AttachmentSweepClaim>();
}

/// Publish a completed R2 object only if this exact completion or recovery
/// claim is still the current monotonic fence.
///
/// The ready-state UPDATE is the irreversible metadata decision and is one
/// atomic CAS. Cleanup follows only after readiness is durable. If cleanup is
/// interrupted, retries see an idempotent ready row; no path can create ready
/// metadata before the caller has independently observed the R2 object.
export async function finalizeAttachmentReadyClaim(
  env: Env,
  claim: AttachmentSweepClaim,
  now: number,
): Promise<AttachmentReadyCompletion> {
  validateClaim(claim);
  const contentExpiresAt = claim.content_expires_at;
  if (
    claim.state !== "completing"
    || !Number.isSafeInteger(contentExpiresAt)
    || contentExpiresAt! <= 0
  ) {
    throw new Error("attachment ready claim state is invalid");
  }
  const ready = await env.DB.prepare(
    `UPDATE attachment_objects
        SET state = 'ready', upload_id = NULL, expires_at = ?
      WHERE id = ?
        AND state = 'completing'
        AND object_key = ?
        AND size_bytes = ?
        AND EXISTS (
          SELECT 1 FROM attachment_sweep_claims AS owned
           WHERE owned.attachment_id = attachment_objects.id
             AND owned.worker_id = ?
             AND owned.claim_token = ?
             AND owned.lease_version = ?
             AND owned.lease_expires_at > ?
        )
     RETURNING id`,
  ).bind(
    contentExpiresAt,
    claim.attachment_id,
    claim.object_key,
    claim.size_bytes,
    claim.worker_id,
    claim.claim_token,
    claim.lease_version,
    now,
  ).first<{ id: string }>();
  if (ready?.id !== claim.attachment_id) return "stale";

  await env.DB.prepare(
    "DELETE FROM attachment_parts WHERE attachment_id = ?",
  ).bind(claim.attachment_id).run();
  await env.DB.prepare(
    `DELETE FROM attachment_sweep_claims
      WHERE attachment_id = ?
        AND worker_id = ?
        AND claim_token = ?
        AND lease_version = ?`,
  ).bind(
    claim.attachment_id,
    claim.worker_id,
    claim.claim_token,
    claim.lease_version,
  ).run();
  return "ready";
}

/// Return a failed multipart completion to `uploading` only while its exact
/// claim still owns the fence. A recovered sweep claim makes this operation a
/// harmless stale result.
export async function releaseAttachmentCompletionClaimAfterFailure(
  env: Env,
  claim: AttachmentSweepClaim,
): Promise<"released" | "stale"> {
  validateClaim(claim);
  const released = await env.DB.prepare(
    `UPDATE attachment_objects
        SET state = 'uploading'
      WHERE id = ?
        AND state = 'completing'
        AND EXISTS (
          SELECT 1 FROM attachment_sweep_claims AS owned
           WHERE owned.attachment_id = attachment_objects.id
             AND owned.worker_id = ?
             AND owned.claim_token = ?
             AND owned.lease_version = ?
        )`,
  ).bind(
    claim.attachment_id,
    claim.worker_id,
    claim.claim_token,
    claim.lease_version,
  ).run();
  if ((released.meta.changes ?? 0) !== 1) return "stale";
  await env.DB.prepare(
    `DELETE FROM attachment_sweep_claims
      WHERE attachment_id = ?
        AND worker_id = ?
        AND claim_token = ?
        AND lease_version = ?`,
  ).bind(
    claim.attachment_id,
    claim.worker_id,
    claim.claim_token,
    claim.lease_version,
  ).run();
  return "released";
}

/// Persist the R2-side absence fence for this exact completing claim.
///
/// The caller invokes this only after a successful multipart abort followed
/// by an empty HEAD, or after deleting a mismatched completed object and
/// re-observing absence. The marker survives failure release and monotonic
/// lease recovery. Metadata deletion below requires it for every `completing`
/// row, so a crash retains quota instead of turning an ambiguous R2 result
/// into a missing attachment row.
export async function confirmAttachmentObjectAbsent(
  env: Env,
  claim: AttachmentSweepClaim,
  now: number,
): Promise<AttachmentStorageFenceCompletion> {
  validateClaim(claim);
  if (
    claim.state !== "completing"
    || !Number.isSafeInteger(now)
    || now <= 0
  ) {
    throw new Error("attachment storage fence input is invalid");
  }
  const confirmed = await env.DB.prepare(
    `UPDATE attachment_sweep_claims
        SET storage_fence_state = 'object_absent_confirmed'
      WHERE attachment_id = ?
        AND worker_id = ?
        AND claim_token = ?
        AND lease_version = ?
        AND lease_expires_at > ?
        AND EXISTS (
          SELECT 1 FROM attachment_objects AS object_row
           WHERE object_row.id = attachment_sweep_claims.attachment_id
             AND object_row.state = 'completing'
        )
      RETURNING attachment_id`,
  ).bind(
    claim.attachment_id,
    claim.worker_id,
    claim.claim_token,
    claim.lease_version,
    now,
  ).first<{ attachment_id: string }>();
  if (confirmed?.attachment_id === claim.attachment_id) return "confirmed";

  const current = await env.DB.prepare(
    `SELECT storage_fence_state
       FROM attachment_sweep_claims
      WHERE attachment_id = ?
        AND worker_id = ?
        AND claim_token = ?
        AND lease_version = ?
        AND lease_expires_at > ?
      LIMIT 1`,
  ).bind(
    claim.attachment_id,
    claim.worker_id,
    claim.claim_token,
    claim.lease_version,
    now,
  ).first<{ storage_fence_state: AttachmentStorageFenceState }>();
  return current?.storage_fence_state === "object_absent_confirmed"
    ? "already_confirmed"
    : "stale";
}

/// Delete metadata only for the exact claim that already completed R2 cleanup.
///
/// Repeating completion after the row was deleted is success. A row that still
/// exists but is no longer owned by this exact Worker/token/version is a stale
/// completion and is refused.
export async function completeAttachmentSweepClaim(
  env: Env,
  claim: AttachmentSweepClaim,
  now: number,
): Promise<AttachmentSweepCompletion> {
  validateClaim(claim);
  const removed = await env.DB.prepare(
    `DELETE FROM attachment_objects
      WHERE id = ?
        AND expires_at <= ?
        AND EXISTS (
          SELECT 1 FROM attachment_sweep_claims AS owned
           WHERE owned.attachment_id = attachment_objects.id
             AND owned.worker_id = ?
             AND owned.claim_token = ?
             AND owned.lease_version = ?
             AND owned.lease_expires_at > ?
             AND (
               attachment_objects.state <> 'completing'
               OR owned.storage_fence_state = 'object_absent_confirmed'
             )
        )
     RETURNING id`,
  ).bind(
    claim.attachment_id,
    now,
    claim.worker_id,
    claim.claim_token,
    claim.lease_version,
    now,
  ).first<{ id: string }>();
  if (removed?.id === claim.attachment_id) return "completed";

  const stillExists = await env.DB.prepare(
    "SELECT 1 AS present FROM attachment_objects WHERE id = ? LIMIT 1",
  ).bind(claim.attachment_id).first<{ present: number }>();
  return stillExists ? "stale" : "already_completed";
}

function retryDelaySeconds(attemptCount: number): number {
  const exponent = Math.min(Math.max(attemptCount - 1, 0), 6);
  return Math.min(
    ATTACHMENT_SWEEP_RETRY_BASE_SECONDS * (2 ** exponent),
    ATTACHMENT_SWEEP_RETRY_MAX_SECONDS,
  );
}

/// Release a failed claim with monotonic version invalidation and backoff.
///
/// A zero-row result is an ordinary stale-worker outcome: another invocation
/// recovered the lease, or an authorized delete already removed the row.
export async function releaseAttachmentSweepClaimAfterFailure(
  env: Env,
  claim: AttachmentSweepClaim,
  now: number,
): Promise<"released" | "stale"> {
  validateClaim(claim);
  const retryNotBefore = now + retryDelaySeconds(claim.attempt_count);
  const released = await env.DB.prepare(
    `UPDATE attachment_sweep_claims
        SET worker_id = NULL,
            claim_token = NULL,
            lease_version = lease_version + 1,
            lease_expires_at = 0,
            retry_not_before = ?
      WHERE attachment_id = ?
        AND worker_id = ?
        AND claim_token = ?
        AND lease_version = ?`,
  ).bind(
    retryNotBefore,
    claim.attachment_id,
    claim.worker_id,
    claim.claim_token,
    claim.lease_version,
  ).run();
  return (released.meta.changes ?? 0) === 1 ? "released" : "stale";
}
