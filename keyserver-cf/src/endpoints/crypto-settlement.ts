/// Trusted callback from the self-hosted node watcher. The watcher discovers
/// payments itself; customers never submit transaction ids or identifying
/// details. A signed retry is idempotent and derives the same license.

import type { Env } from "../env.js";
import {
  encryptLicenseForDelivery,
  getAnonymousInvoice,
} from "../lib/anonymous-crypto.js";
import {
  sha256Hex,
  type WatcherSettlementEvidence,
  verifyWatcherSettlementSignature,
} from "../lib/crypto-watcher-auth.js";
import { badRequest, conflict, json, serviceUnavailable, unauthorized } from "../lib/http.js";
import { generateInvoiceLicenseKey } from "../lib/license.js";
import { notifyTelegramForCryptoSettlement } from "../lib/telegram.js";

const DELIVERY_RETENTION = 7 * 24 * 60 * 60;
const CONFIRMATION_GRACE = 24 * 60 * 60;
const PRO_LIFETIME_USD_CENTS = 500;

export async function handleCryptoSettlement(
  request: Request,
  env: Env,
  ctx?: Pick<ExecutionContext, "waitUntil">,
): Promise<Response> {
  if (!env.CRYPTO_WATCHER_SETTLEMENT_PUBLIC_KEY || !env.LICENSE_HMAC_SECRET) {
    return serviceUnavailable("crypto settlement is not configured");
  }
  const raw = await request.text();
  let body: WatcherSettlementEvidence & {
    event_id?: unknown;
    invoice_id?: unknown;
    payment_method?: unknown;
    amount_atomic?: unknown;
    confirmations?: unknown;
    observed_at?: unknown;
    payment_reference_commitment?: unknown;
  };
  try {
    body = JSON.parse(raw) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (typeof body.event_id !== "string" || !/^evt_[0-9a-f]{32,64}$/.test(body.event_id)) {
    return badRequest("event_id malformed");
  }
  if (typeof body.invoice_id !== "string" || !/^cpay_[0-9a-f]{32}$/.test(body.invoice_id)) {
    return badRequest("invoice_id malformed");
  }
  if ((body.payment_method !== "btc" && body.payment_method !== "xmr") ||
      typeof body.amount_atomic !== "string" || !/^[1-9]\d{0,30}$/.test(body.amount_atomic) ||
      typeof body.confirmations !== "number" || !Number.isSafeInteger(body.confirmations) ||
      body.confirmations < 1 ||
      typeof body.observed_at !== "number" || !Number.isSafeInteger(body.observed_at) ||
      typeof body.payment_reference_commitment !== "string" ||
      !/^[0-9a-f]{64}$/.test(body.payment_reference_commitment)) {
    return badRequest("settlement evidence malformed");
  }
  const evidence = body as WatcherSettlementEvidence;
  const path = new URL(request.url).pathname;
  if (!(await verifyWatcherSettlementSignature(
    request.headers,
    env.CRYPTO_WATCHER_SETTLEMENT_PUBLIC_KEY,
    request.method,
    path,
    evidence,
  ))) {
    return unauthorized("invalid watcher signature");
  }
  const expectedEventId = `evt_${await sha256Hex(
    `${evidence.invoice_id}:${evidence.payment_method}:${evidence.payment_reference_commitment}`,
  )}`;
  if (evidence.event_id !== expectedEventId) {
    return badRequest("event_id does not match payment proof");
  }

  const invoice = await getAnonymousInvoice(env.DB, body.invoice_id);
  if (!invoice) return badRequest("unknown invoice");
  if (
    invoice.status !== "pending" &&
    invoice.status !== "paid" &&
    invoice.status !== "delivery_ready"
  ) {
    return conflict(`invoice is ${invoice.status}`);
  }
  if (invoice.status === "paid" && invoice.settlement_event_id !== body.event_id) {
    return conflict("invoice is already processing a different settlement");
  }
  if (invoice.plan !== "pro" || invoice.amount_usd_cents !== PRO_LIFETIME_USD_CENTS) {
    return conflict("invoice does not represent lifetime Pro");
  }
  const requiredConfirmations = invoice.confirmations_required;
  if (!Number.isSafeInteger(requiredConfirmations) || requiredConfirmations < 1 ||
      body.confirmations < requiredConfirmations) {
    return conflict("payment does not have enough confirmations");
  }
  if (invoice.payment_method !== body.payment_method ||
      BigInt(body.amount_atomic) < BigInt(invoice.amount_atomic)) {
    return conflict("payment does not satisfy this invoice");
  }
  const nowSeconds = Math.floor(Date.now() / 1000);
  if (body.observed_at < invoice.created_at || body.observed_at > invoice.expires_at) {
    return conflict("payment was observed outside the invoice window");
  }
  if (body.observed_at > nowSeconds + 300) {
    return conflict("payment observation time is in the future");
  }

  // Claim the on-chain payment reference before issuing anything. The signer
  // binds invoice_id into the callback, while this D1 uniqueness boundary also
  // prevents a watcher retry or attribution bug from applying one payment to a
  // second invoice. A retry for the same invoice/event remains idempotent.
  const referenceClaim = await env.DB.prepare(
    `INSERT OR IGNORE INTO crypto_payment_references_v2 (
       payment_method, payment_reference_commitment, invoice_id, event_id, claimed_at
     ) VALUES (?, ?, ?, ?, ?)`,
  ).bind(
    body.payment_method,
    body.payment_reference_commitment,
    invoice.invoice_id,
    body.event_id,
    Math.floor(Date.now() / 1000),
  ).run();
  if ((referenceClaim.meta?.changes ?? 0) === 0) {
    const existing = await env.DB.prepare(
      `SELECT invoice_id, event_id FROM crypto_payment_references_v2
        WHERE payment_method = ? AND payment_reference_commitment = ?`,
    ).bind(
      body.payment_method,
      body.payment_reference_commitment,
    ).first<{ invoice_id: string; event_id: string }>();
    if (existing?.invoice_id !== invoice.invoice_id || existing.event_id !== body.event_id) {
      return conflict("payment reference is already assigned to another invoice");
    }
  }
  if (invoice.status === "delivery_ready") {
    return json({ ok: true, duplicate: true, status: "delivery_ready" });
  }

  const license = await generateInvoiceLicenseKey(env.LICENSE_HMAC_SECRET, invoice.invoice_id);
  const encryptedLicense = await encryptLicenseForDelivery(
    invoice.delivery_public_key_spki,
    license.plaintext,
  );
  const now = Math.floor(Date.now() / 1000);
  const subscriptionId = `crypto_${invoice.invoice_id}`;
  const customerId = `crypto:${invoice.invoice_id}`;

  try {
    const claimed = await env.DB.prepare(
      `UPDATE crypto_invoices_v2
          SET status = 'paid', settlement_event_id = ?
        WHERE invoice_id = ? AND status = 'pending'`,
    ).bind(body.event_id, invoice.invoice_id).run();
    if ((claimed.meta?.changes ?? 0) === 0) {
      const current = await getAnonymousInvoice(env.DB, invoice.invoice_id);
      if (current?.status === "delivery_ready") {
        return json({ ok: true, duplicate: true, status: "delivery_ready" });
      }
      if (current?.status !== "paid" || current.settlement_event_id !== body.event_id) {
        return conflict(`invoice is ${current?.status ?? "missing"}`);
      }
    }
    const results = await env.DB.batch([
      env.DB.prepare(
        `INSERT OR IGNORE INTO crypto_settlement_events_v2
          (event_id, invoice_id, processed_at) VALUES (?, ?, ?)`,
      ).bind(body.event_id, invoice.invoice_id, now),
      env.DB.prepare(
        `INSERT INTO subscriptions
          (subscription_id, customer_id, customer_email, status,
           current_period_end, cancel_at_period_end, created_at, updated_at, is_comp)
         VALUES (?, ?, '', 'ACTIVE', ?, 0, ?, ?, 0)
         ON CONFLICT(subscription_id) DO UPDATE SET
           status = 'ACTIVE', current_period_end = excluded.current_period_end,
           updated_at = excluded.updated_at`,
      ).bind(subscriptionId, customerId, null, now, now),
      env.DB.prepare(
        `INSERT OR IGNORE INTO licenses
          (license_hash, subscription_id, issued_at) VALUES (?, ?, ?)`,
      ).bind(license.hash, subscriptionId, now),
      env.DB.prepare(
        `UPDATE crypto_invoices_v2 SET
          status = 'delivery_ready',
          encrypted_license = ?, resolved_at = ?, cleanup_at = ?
         WHERE invoice_id = ? AND status = 'paid' AND settlement_event_id = ?`,
      ).bind(
        encryptedLicense,
        now,
        now + DELIVERY_RETENTION,
        invoice.invoice_id,
        body.event_id,
      ),
    ]);
    const finalized = (results.at(-1)?.meta?.changes ?? 0) > 0;
    if (finalized) {
      const notification = notifyTelegramForCryptoSettlement(
        env,
        invoice.payment_method,
        invoice.amount_usd_cents,
      ).catch(() => console.error("[crypto-settlement] Telegram alert failed"));
      if (ctx) ctx.waitUntil(notification);
      else await notification;
    }
    return json({ ok: true, duplicate: !finalized, status: "delivery_ready" });
  } catch {
    console.error("[crypto-settlement] issuance transaction failed");
    return serviceUnavailable("entitlement issuance failed");
  }
}

export async function sweepAnonymousCryptoInvoices(db: D1Database): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  const expired = await db.prepare(
    "UPDATE crypto_invoices_v2 SET status = 'expired' WHERE status = 'pending' AND expires_at < ?",
  ).bind(now - CONFIRMATION_GRACE).run();
  const deleted = await db.prepare(
    "DELETE FROM crypto_invoices_v2 WHERE cleanup_at < ?",
  ).bind(now).run();
  await db.prepare(
    "DELETE FROM crypto_settlement_events_v2 WHERE processed_at < ?",
  ).bind(now - DELIVERY_RETENTION).run();
  return (expired.meta?.changes ?? 0) + (deleted.meta?.changes ?? 0);
}
