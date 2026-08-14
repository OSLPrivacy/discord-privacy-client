import type { Env } from "../env.js";
import type { AttachmentSweepClaim } from "./attachment-sweep-claims.js";
import {
  ATTACHMENT_SWEEP_RETRY_BASE_SECONDS,
  ATTACHMENT_SWEEP_RETRY_MAX_SECONDS,
} from "./attachment-sweep-claims.js";

export const RETENTION_TERMINAL_CONFIRMATIONS = 3;
const REPORT_LEASE_SECONDS = 60;
const REPORT_RETRY_MAX_SECONDS = 60 * 60;
const TELEGRAM_CHAT_ID_RE = /^-?[1-9][0-9]{0,19}$/;
const TELEGRAM_BOT_TOKEN_RE = /^[A-Za-z0-9:_-]{20,128}$/;

export class UnrecoverableRetentionCleanupError extends Error {
  readonly retentionTerminalReason: string;

  constructor(reason: string) {
    if (!/^[a-z][a-z0-9_]{2,63}$/.test(reason)) {
      throw new Error("retention terminal reason is invalid");
    }
    super(reason);
    this.name = "UnrecoverableRetentionCleanupError";
    this.retentionTerminalReason = reason;
  }
}

function retryDelay(attemptCount: number): number {
  const exponent = Math.min(Math.max(attemptCount - 1, 0), 6);
  return Math.min(
    ATTACHMENT_SWEEP_RETRY_BASE_SECONDS * (2 ** exponent),
    ATTACHMENT_SWEEP_RETRY_MAX_SECONDS,
  );
}

function terminalReason(error: unknown): string | null {
  if (error instanceof UnrecoverableRetentionCleanupError) {
    return error.retentionTerminalReason;
  }
  const code = typeof error === "object" && error !== null && "code" in error
    ? String((error as { code: unknown }).code)
    : "";
  switch (code) {
    case "object_locked_permanent":
    case "retention_policy_invalid":
    case "authorization_revoked":
      return `provider_${code}`;
    default:
      return null;
  }
}

export type RetentionFailureDisposition = "retry" | "terminal" | "stale";

/**
 * Persist one cleanup failure against the exact leased claim.
 *
 * Timeouts, transient provider denial, lost receipts, and unknown failures
 * always take the retry path.  A terminal report becomes eligible only after
 * the same explicit unrecoverable reason is independently observed three
 * times.  No acknowledgement or person is part of this state transition.
 */
export async function recordAttachmentCleanupFailure(
  env: Env,
  claim: AttachmentSweepClaim,
  error: unknown,
  now: number,
): Promise<RetentionFailureDisposition> {
  const reason = terminalReason(error);
  const retryNotBefore = now + retryDelay(claim.attempt_count);
  if (reason === null) {
    const released = await env.DB.prepare(
      `UPDATE attachment_sweep_claims
          SET worker_id = NULL, claim_token = NULL,
              lease_version = lease_version + 1, lease_expires_at = 0,
              retry_not_before = ?, unrecoverable_reason = NULL,
              unrecoverable_count = 0
        WHERE attachment_id = ? AND worker_id = ? AND claim_token = ?
          AND lease_version = ? AND terminal_at IS NULL`,
    ).bind(
      retryNotBefore,
      claim.attachment_id,
      claim.worker_id,
      claim.claim_token,
      claim.lease_version,
    ).run();
    return (released.meta.changes ?? 0) === 1 ? "retry" : "stale";
  }

  const updated = await env.DB.prepare(
    `UPDATE attachment_sweep_claims
        SET worker_id = NULL, claim_token = NULL,
            lease_version = lease_version + 1, lease_expires_at = 0,
            retry_not_before = ?,
            unrecoverable_count = CASE
              WHEN unrecoverable_reason = ? THEN MIN(unrecoverable_count + 1, ?)
              ELSE 1
            END,
            unrecoverable_reason = ?,
            terminal_at = CASE
              WHEN (CASE WHEN unrecoverable_reason = ?
                         THEN unrecoverable_count + 1 ELSE 1 END) >= ?
                THEN ? ELSE NULL END,
            report_status = CASE
              WHEN (CASE WHEN unrecoverable_reason = ?
                         THEN unrecoverable_count + 1 ELSE 1 END) >= ?
                THEN 'pending' ELSE 'none' END,
            report_next_attempt_at = CASE
              WHEN (CASE WHEN unrecoverable_reason = ?
                         THEN unrecoverable_count + 1 ELSE 1 END) >= ?
                THEN ? ELSE report_next_attempt_at END
      WHERE attachment_id = ? AND worker_id = ? AND claim_token = ?
        AND lease_version = ? AND terminal_at IS NULL
      RETURNING terminal_at`,
  ).bind(
    retryNotBefore,
    reason,
    RETENTION_TERMINAL_CONFIRMATIONS,
    reason,
    reason,
    RETENTION_TERMINAL_CONFIRMATIONS,
    now,
    reason,
    RETENTION_TERMINAL_CONFIRMATIONS,
    reason,
    RETENTION_TERMINAL_CONFIRMATIONS,
    now,
    claim.attachment_id,
    claim.worker_id,
    claim.claim_token,
    claim.lease_version,
  ).first<{ terminal_at: number | null }>();
  if (!updated) return "stale";
  return updated.terminal_at === null ? "retry" : "terminal";
}

interface TerminalReportRow {
  attachment_id: string;
  cleanup_policy: string;
  unrecoverable_reason: string;
  attempt_count: number;
  expires_at: number;
}

function ownerChatIds(env: Env): string[] {
  const raw = env.TELEGRAM_OPERATOR_CHAT_IDS ?? env.TELEGRAM_ADMIN_CHAT_ID ?? "";
  const ids = [...new Set(raw.split(",").map((value) => value.trim()).filter(Boolean))];
  if (ids.length === 0 || ids.length > 16 || ids.some((id) => !TELEGRAM_CHAT_ID_RE.test(id))) {
    return [];
  }
  return ids;
}

export function retentionTerminalReportText(row: TerminalReportRow): string {
  return [
    "OSL retention cleanup terminal",
    `policy=${row.cleanup_policy}`,
    `object=${row.attachment_id}`,
    `oldest_due_item=${row.attachment_id}`,
    `oldest_due_at=${row.expires_at}`,
    `attempts=${row.attempt_count}`,
    `terminal_reason=${row.unrecoverable_reason}`,
  ].join("\n");
}

async function sendTelegramOwnerReport(
  env: Env,
  text: string,
  fetcher: typeof fetch,
): Promise<void> {
  const token = env.TELEGRAM_BOT_TOKEN ?? "";
  const chatIds = ownerChatIds(env);
  if (!TELEGRAM_BOT_TOKEN_RE.test(token) || chatIds.length === 0) {
    throw new Error("Telegram owner report path is not configured");
  }
  for (const chatId of chatIds) {
    const response = await fetcher(`https://api.telegram.org/bot${token}/sendMessage`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ chat_id: chatId, text, disable_web_page_preview: true }),
      signal: AbortSignal.timeout(5_000),
    });
    if (!response.ok) throw new Error(`Telegram sendMessage returned ${response.status}`);
  }
}

export interface TerminalReportDrainResult {
  attempted: number;
  delivered: number;
}

/** Drain terminal reports through the existing Telegram Bot API owner path. */
export async function drainTerminalRetentionReports(
  env: Env,
  fetcher: typeof fetch = fetch,
  now = Math.floor(Date.now() / 1000),
): Promise<TerminalReportDrainResult> {
  const row = await env.DB.prepare(
    `SELECT claim.attachment_id, claim.cleanup_policy,
            claim.unrecoverable_reason, claim.attempt_count,
            object_row.expires_at
       FROM attachment_sweep_claims AS claim
       JOIN attachment_objects AS object_row
         ON object_row.id = claim.attachment_id
      WHERE claim.report_status = 'pending'
        AND claim.report_next_attempt_at <= ?
      ORDER BY object_row.expires_at, claim.attachment_id
      LIMIT 1`,
  ).bind(now).first<TerminalReportRow>();
  if (!row) return { attempted: 0, delivered: 0 };

  const leased = await env.DB.prepare(
    `UPDATE attachment_sweep_claims SET report_next_attempt_at = ?
      WHERE attachment_id = ? AND report_status = 'pending'
        AND report_next_attempt_at <= ?`,
  ).bind(now + REPORT_LEASE_SECONDS, row.attachment_id, now).run();
  if ((leased.meta.changes ?? 0) !== 1) return { attempted: 0, delivered: 0 };

  try {
    await sendTelegramOwnerReport(env, retentionTerminalReportText(row), fetcher);
    await env.DB.prepare(
      `UPDATE attachment_sweep_claims
          SET report_status = 'delivered', report_attempts = report_attempts + 1,
              report_delivered_at = ?, report_next_attempt_at = ?
        WHERE attachment_id = ? AND report_status = 'pending'`,
    ).bind(now, now, row.attachment_id).run();
    return { attempted: 1, delivered: 1 };
  } catch {
    const current = await env.DB.prepare(
      `SELECT report_attempts FROM attachment_sweep_claims
        WHERE attachment_id = ?`,
    ).bind(row.attachment_id).first<{ report_attempts: number }>();
    const attempts = (current?.report_attempts ?? 0) + 1;
    const delay = Math.min(REPORT_RETRY_MAX_SECONDS, 30 * (2 ** Math.min(attempts - 1, 7)));
    await env.DB.prepare(
      `UPDATE attachment_sweep_claims
          SET report_attempts = ?, report_next_attempt_at = ?
        WHERE attachment_id = ? AND report_status = 'pending'`,
    ).bind(attempts, now + delay, row.attachment_id).run();
    return { attempted: 1, delivered: 0 };
  }
}
