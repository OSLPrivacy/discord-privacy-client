import type { Env } from "../env.js";
import {
  notifyTelegramForCryptoDonation,
  notifyTelegramForCryptoSettlement,
  telegramOperatorAlertsAreConfigured,
} from "./telegram.js";

export type PaymentAlertKind = "crypto_pro" | "crypto_donation";
export type CryptoPaymentMethod = "btc" | "xmr";

interface PaymentAlertRow {
  alert_id: string;
  alert_kind: PaymentAlertKind;
  payment_method: CryptoPaymentMethod;
  amount_usd_cents: number;
}

export interface PaymentAlertDrainResult {
  configured: boolean;
  attempted: number;
  delivered: number;
}

const ALERT_PREFIX = "payalert_";
const MAX_BATCH = 20;
const DELIVERY_LEASE_SECONDS = 60;
const MAX_BACKOFF_SECONDS = 60 * 60;
const DELIVERED_RETENTION_SECONDS = 7 * 24 * 60 * 60;

function bytesToHex(bytes: ArrayBuffer): string {
  return [...new Uint8Array(bytes)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * One-way, deterministic idempotency key. `sourceId` is used only as hash
 * input and is never returned to or retained by the outbox.
 */
export async function paymentAlertId(
  kind: PaymentAlertKind,
  sourceId: string,
): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(`osl-payment-alert-v1\0${kind}\0${sourceId}`),
  );
  return `${ALERT_PREFIX}${bytesToHex(digest)}`;
}

export async function paymentAlertInsertStatement(
  db: D1Database,
  kind: PaymentAlertKind,
  sourceId: string,
  paymentMethod: CryptoPaymentMethod,
  amountUsdCents: number,
  now = Math.floor(Date.now() / 1000),
): Promise<D1PreparedStatement> {
  const alertId = await paymentAlertId(kind, sourceId);
  return db.prepare(
    `INSERT INTO payment_alert_outbox
       (alert_id, alert_kind, payment_method, amount_usd_cents,
        status, attempts, next_attempt_at, created_at)
     VALUES (?, ?, ?, ?, 'pending', 0, ?, ?)
     ON CONFLICT(alert_id) DO UPDATE SET
       next_attempt_at = CASE
         WHEN payment_alert_outbox.status = 'pending'
           THEN MIN(payment_alert_outbox.next_attempt_at, excluded.next_attempt_at)
         ELSE payment_alert_outbox.next_attempt_at
       END
     WHERE payment_alert_outbox.alert_kind = excluded.alert_kind
       AND payment_alert_outbox.payment_method = excluded.payment_method
       AND payment_alert_outbox.amount_usd_cents = excluded.amount_usd_cents`,
  ).bind(alertId, kind, paymentMethod, amountUsdCents, now, now);
}

export async function ensurePaymentAlert(
  db: D1Database,
  kind: PaymentAlertKind,
  sourceId: string,
  paymentMethod: CryptoPaymentMethod,
  amountUsdCents: number,
  now = Math.floor(Date.now() / 1000),
): Promise<string> {
  const alertId = await paymentAlertId(kind, sourceId);
  await (await paymentAlertInsertStatement(
    db,
    kind,
    sourceId,
    paymentMethod,
    amountUsdCents,
    now,
  )).run();
  const row = await db.prepare(
    `SELECT alert_kind, payment_method, amount_usd_cents
       FROM payment_alert_outbox WHERE alert_id = ?`,
  ).bind(alertId).first<{
    alert_kind: string;
    payment_method: string;
    amount_usd_cents: number;
  }>();
  if (
    row?.alert_kind !== kind ||
    row.payment_method !== paymentMethod ||
    row.amount_usd_cents !== amountUsdCents
  ) {
    throw new Error("payment alert idempotency conflict");
  }
  return alertId;
}

async function deliverAlert(
  env: Env,
  row: PaymentAlertRow,
  fetcher: typeof fetch,
): Promise<void> {
  if (row.alert_kind === "crypto_pro") {
    await notifyTelegramForCryptoSettlement(
      env,
      row.payment_method,
      row.amount_usd_cents,
      fetcher,
    );
    return;
  }
  await notifyTelegramForCryptoDonation(
    env,
    row.payment_method,
    row.amount_usd_cents,
    fetcher,
  );
}

function retryDelay(attemptNumber: number): number {
  return Math.min(MAX_BACKOFF_SECONDS, 30 * (2 ** Math.min(attemptNumber - 1, 7)));
}

/**
 * Drain due alerts with a short D1 lease. Delivery is intentionally
 * at-least-once: a Worker crash after Telegram accepts a message but before D1
 * records success may repeat an operator alert, but can never silently lose it.
 */
export async function drainPaymentAlertOutbox(
  env: Env,
  fetcher: typeof fetch = fetch,
  now = Math.floor(Date.now() / 1000),
): Promise<PaymentAlertDrainResult> {
  if (!telegramOperatorAlertsAreConfigured(env)) {
    return { configured: false, attempted: 0, delivered: 0 };
  }

  const due = await env.DB.prepare(
    `SELECT alert_id, alert_kind, payment_method, amount_usd_cents
       FROM payment_alert_outbox
      WHERE status = 'pending' AND next_attempt_at <= ?
      ORDER BY created_at, alert_id
      LIMIT ?`,
  ).bind(now, MAX_BATCH).all<PaymentAlertRow>();

  let attempted = 0;
  let delivered = 0;
  for (const row of due.results) {
    const leased = await env.DB.prepare(
      `UPDATE payment_alert_outbox
          SET next_attempt_at = ?
        WHERE alert_id = ? AND status = 'pending' AND next_attempt_at <= ?`,
    ).bind(now + DELIVERY_LEASE_SECONDS, row.alert_id, now).run();
    if ((leased.meta?.changes ?? 0) === 0) continue;
    attempted += 1;

    try {
      await deliverAlert(env, row, fetcher);
      const result = await env.DB.prepare(
        `UPDATE payment_alert_outbox
            SET status = 'delivered', attempts = attempts + 1,
                delivered_at = ?, next_attempt_at = ?
          WHERE alert_id = ? AND status = 'pending'`,
      ).bind(now, now, row.alert_id).run();
      if ((result.meta?.changes ?? 0) > 0) delivered += 1;
    } catch {
      const current = await env.DB.prepare(
        "SELECT attempts FROM payment_alert_outbox WHERE alert_id = ?",
      ).bind(row.alert_id).first<{ attempts: number }>();
      const attemptNumber = (current?.attempts ?? 0) + 1;
      await env.DB.prepare(
        `UPDATE payment_alert_outbox
            SET attempts = ?, next_attempt_at = ?
          WHERE alert_id = ? AND status = 'pending'`,
      ).bind(attemptNumber, now + retryDelay(attemptNumber), row.alert_id).run();
    }
  }
  return { configured: true, attempted, delivered };
}

export async function sweepDeliveredPaymentAlerts(
  db: D1Database,
  now = Math.floor(Date.now() / 1000),
): Promise<number> {
  const result = await db.prepare(
    `DELETE FROM payment_alert_outbox
      WHERE status = 'delivered' AND delivered_at < ?`,
  ).bind(now - DELIVERED_RETENTION_SECONDS).run();
  return result.meta?.changes ?? 0;
}
