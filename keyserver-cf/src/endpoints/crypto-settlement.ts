/// Trusted callback from the self-hosted node watcher. The watcher discovers
/// payments itself; customers never submit transaction ids or identifying
/// details. A signed retry is idempotent and derives the same license.

import type { Env } from "../env.js";
import {
  encryptLicenseForDelivery,
  getAnonymousInvoice,
} from "../lib/anonymous-crypto.js";
import { getCryptoDonationInvoice } from "../lib/crypto-donations.js";
import {
  sha256Hex,
  type WatcherSettlementEvidence,
  verifyWatcherSettlementSignature,
} from "../lib/crypto-watcher-auth.js";
import { badRequest, conflict, json, serviceUnavailable, unauthorized } from "../lib/http.js";
import { generateInvoiceLicenseKey } from "../lib/license.js";
import { isDonationAmount } from "../lib/donations.js";
import {
  notifyTelegramForCryptoDonation,
  notifyTelegramForCryptoSettlement,
} from "../lib/telegram.js";

const DELIVERY_RETENTION = 7 * 24 * 60 * 60;
const CONFIRMATION_GRACE = 24 * 60 * 60;
const PRO_LIFETIME_USD_CENTS = 500;

export async function handleCryptoSettlement(
  request: Request,
  env: Env,
  ctx?: Pick<ExecutionContext, "waitUntil">,
): Promise<Response> {
  if (!env.CRYPTO_WATCHER_SETTLEMENT_PUBLIC_KEY) {
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
  if (typeof body.invoice_id !== "string" || !/^(?:cpay|cdon)_[0-9a-f]{32}$/.test(body.invoice_id)) {
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

  if (body.invoice_id.startsWith("cdon_")) {
    return await settleCryptoDonation(body as WatcherSettlementEvidence, env, ctx);
  }

  if (!env.LICENSE_HMAC_SECRET) {
    return serviceUnavailable("crypto entitlement settlement is not configured");
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
  const referenceConflict = await claimPaymentReference(env.DB, evidence);
  if (referenceConflict) return referenceConflict;
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
        `INSERT OR IGNORE INTO crypto_commerce_events
          (subscription_id, payment_method, amount_usd_cents, settled_at)
         VALUES (?, ?, ?, ?)`,
      ).bind(subscriptionId, invoice.payment_method, invoice.amount_usd_cents, now),
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

async function settleCryptoDonation(
  evidence: WatcherSettlementEvidence,
  env: Env,
  ctx?: Pick<ExecutionContext, "waitUntil">,
): Promise<Response> {
  const invoice = await getCryptoDonationInvoice(env.DB, evidence.invoice_id);
  if (!invoice) return badRequest("unknown invoice");
  if (invoice.status !== "pending" && invoice.status !== "paid" && invoice.status !== "recorded") {
    return conflict(`invoice is ${invoice.status}`);
  }
  if (invoice.status === "paid" && invoice.settlement_event_id !== evidence.event_id) {
    return conflict("invoice is already processing a different settlement");
  }
  if (!isDonationAmount(invoice.amount_usd_cents)) {
    return conflict("invoice does not represent a bounded donation");
  }
  if (
    !Number.isSafeInteger(invoice.confirmations_required) ||
    invoice.confirmations_required < 1 ||
    evidence.confirmations < invoice.confirmations_required
  ) {
    return conflict("payment does not have enough confirmations");
  }
  if (
    invoice.payment_method !== evidence.payment_method ||
    BigInt(evidence.amount_atomic) < BigInt(invoice.amount_atomic)
  ) {
    return conflict("payment does not satisfy this invoice");
  }
  const nowSeconds = Math.floor(Date.now() / 1000);
  if (evidence.observed_at < invoice.created_at || evidence.observed_at > invoice.expires_at) {
    return conflict("payment was observed outside the invoice window");
  }
  if (evidence.observed_at > nowSeconds + 300) {
    return conflict("payment observation time is in the future");
  }

  const referenceConflict = await claimPaymentReference(env.DB, evidence);
  if (referenceConflict) return referenceConflict;
  if (invoice.status === "recorded") {
    return json({ ok: true, duplicate: true, status: "recorded" });
  }

  const now = Math.floor(Date.now() / 1000);
  try {
    const claimed = await env.DB.prepare(
      `UPDATE crypto_donation_invoices
          SET status = 'paid', settlement_event_id = ?
        WHERE invoice_id = ? AND status = 'pending'`,
    ).bind(evidence.event_id, invoice.invoice_id).run();
    if ((claimed.meta?.changes ?? 0) === 0) {
      const current = await getCryptoDonationInvoice(env.DB, invoice.invoice_id);
      if (current?.status === "recorded") {
        return json({ ok: true, duplicate: true, status: "recorded" });
      }
      if (current?.status !== "paid" || current.settlement_event_id !== evidence.event_id) {
        return conflict(`invoice is ${current?.status ?? "missing"}`);
      }
    }
    const results = await env.DB.batch([
      env.DB.prepare(
        `INSERT OR IGNORE INTO crypto_settlement_events_v2
          (event_id, invoice_id, processed_at) VALUES (?, ?, ?)`,
      ).bind(evidence.event_id, invoice.invoice_id, now),
      env.DB.prepare(
        `INSERT OR IGNORE INTO crypto_donation_events
          (donation_id, payment_method, amount_usd_cents, settled_at)
         VALUES (?, ?, ?, ?)`,
      ).bind(
        `crypto_${invoice.invoice_id}`,
        invoice.payment_method,
        invoice.amount_usd_cents,
        now,
      ),
      env.DB.prepare(
        `UPDATE crypto_donation_invoices SET
           status = 'recorded', resolved_at = ?, cleanup_at = ?
         WHERE invoice_id = ? AND status = 'paid' AND settlement_event_id = ?
           AND EXISTS (
             SELECT 1 FROM crypto_donation_events
              WHERE donation_id = ? AND payment_method = ? AND amount_usd_cents = ?
           )`,
      ).bind(
        now,
        now + DELIVERY_RETENTION,
        invoice.invoice_id,
        evidence.event_id,
        `crypto_${invoice.invoice_id}`,
        invoice.payment_method,
        invoice.amount_usd_cents,
      ),
    ]);
    const finalized = (results.at(-1)?.meta?.changes ?? 0) > 0;
    if (finalized) {
      const notification = notifyTelegramForCryptoDonation(
        env,
        invoice.payment_method,
        invoice.amount_usd_cents,
      ).catch(() => console.error("[crypto-donation] Telegram alert failed"));
      if (ctx) ctx.waitUntil(notification);
      else await notification;
    } else {
      const current = await getCryptoDonationInvoice(env.DB, invoice.invoice_id);
      if (current?.status !== "recorded") {
        console.error("[crypto-donation] durable donation event conflict");
        return serviceUnavailable("donation recording failed");
      }
    }
    return json({ ok: true, duplicate: !finalized, status: "recorded" });
  } catch {
    console.error("[crypto-donation] recording transaction failed");
    return serviceUnavailable("donation recording failed");
  }
}

async function claimPaymentReference(
  db: D1Database,
  evidence: WatcherSettlementEvidence,
): Promise<Response | null> {
  // This table deliberately has no invoice-table foreign key or prefix check:
  // it is the global boundary shared by cpay_ entitlements and cdon_ donations.
  const referenceClaim = await db.prepare(
    `INSERT OR IGNORE INTO crypto_payment_references_v2 (
       payment_method, payment_reference_commitment, invoice_id, event_id, claimed_at
     ) VALUES (?, ?, ?, ?, ?)`,
  ).bind(
    evidence.payment_method,
    evidence.payment_reference_commitment,
    evidence.invoice_id,
    evidence.event_id,
    Math.floor(Date.now() / 1000),
  ).run();
  if ((referenceClaim.meta?.changes ?? 0) > 0) return null;
  const existing = await db.prepare(
    `SELECT invoice_id, event_id FROM crypto_payment_references_v2
      WHERE payment_method = ? AND payment_reference_commitment = ?`,
  ).bind(
    evidence.payment_method,
    evidence.payment_reference_commitment,
  ).first<{ invoice_id: string; event_id: string }>();
  if (existing?.invoice_id !== evidence.invoice_id || existing.event_id !== evidence.event_id) {
    return conflict("payment reference is already assigned to another invoice");
  }
  return null;
}

export async function sweepAnonymousCryptoInvoices(db: D1Database): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  const expired = await db.prepare(
    "UPDATE crypto_invoices_v2 SET status = 'expired' WHERE status = 'pending' AND expires_at < ?",
  ).bind(now - CONFIRMATION_GRACE).run();
  const deleted = await db.prepare(
    "DELETE FROM crypto_invoices_v2 WHERE cleanup_at < ?",
  ).bind(now).run();
  const expiredDonations = await db.prepare(
    `UPDATE crypto_donation_invoices SET status = 'expired'
      WHERE status = 'pending' AND expires_at < ?`,
  ).bind(now - CONFIRMATION_GRACE).run();
  const deletedDonations = await db.prepare(
    "DELETE FROM crypto_donation_invoices WHERE cleanup_at < ?",
  ).bind(now).run();
  await db.prepare(
    "DELETE FROM crypto_settlement_events_v2 WHERE processed_at < ?",
  ).bind(now - DELIVERY_RETENTION).run();
  return (expired.meta?.changes ?? 0) +
    (deleted.meta?.changes ?? 0) +
    (expiredDonations.meta?.changes ?? 0) +
    (deletedDonations.meta?.changes ?? 0);
}
