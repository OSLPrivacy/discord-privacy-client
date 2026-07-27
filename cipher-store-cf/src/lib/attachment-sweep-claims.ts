import type { Env } from "../env.js";

export const ATTACHMENT_SWEEP_LEASE_SECONDS = 2 * 60;
export const ATTACHMENT_SWEEP_RETRY_BASE_SECONDS = 5 * 60;
export const ATTACHMENT_SWEEP_RETRY_MAX_SECONDS = 6 * 60 * 60;

const WORKER_ID_RE = /^[0-9a-f]{32}$/;
const CLAIM_TOKEN_RE = /^[0-9a-f]{64}$/;

export interface AttachmentSweepClaim {
  attachment_id: string;
  object_key: string;
  upload_id: string | null;
  worker_id: string;
  claim_token: string;
  lease_version: number;
  lease_expires_at: number;
  attempt_count: number;
}

interface ClaimedIdentity {
  attachment_id: string;
  worker_id: string;
  claim_token: string;
  lease_version: number;
  lease_expires_at: number;
  attempt_count: number;
}

export type AttachmentSweepCompletion =
  | "completed"
  | "already_completed"
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
/// relies on so an absent or partial 0008 schema refuses before legacy expiry
/// marking, R2 cleanup, or attachment metadata deletion.
export async function requireAttachmentSweepClaimSchema(env: Env): Promise<void> {
  await env.DB.prepare(
    `SELECT attachment_id, worker_id, claim_token, lease_version,
            lease_expires_at, retry_not_before, attempt_count, last_claimed_at
       FROM attachment_sweep_claims
      LIMIT 0`,
  ).all();
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
        lease_expires_at, retry_not_before, attempt_count, last_claimed_at)
     SELECT candidate.id, ?, ?, 1, ?, 0, 1, ?
       FROM attachment_objects AS candidate
       LEFT JOIN attachment_sweep_claims AS existing
         ON existing.attachment_id = candidate.id
      WHERE candidate.expires_at <= ?
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
               lease_expires_at, attempt_count`,
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
            object_row.upload_id,
            claim.worker_id,
            claim.claim_token,
            claim.lease_version,
            claim.lease_expires_at,
            claim.attempt_count
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
