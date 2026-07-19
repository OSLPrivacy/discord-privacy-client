import type { CryptoAsset } from "./anonymous-crypto.js";
import { sha256Hex } from "./crypto-watcher-auth.js";

export interface CryptoDonationInvoiceRow {
  invoice_id: string;
  claim_hash: string;
  payment_method: CryptoAsset;
  amount_usd_cents: number;
  amount_atomic: string;
  confirmations_required: number;
  price_locked_at: number;
  status: "pending" | "paid" | "recorded" | "expired";
  settlement_event_id: string | null;
  created_at: number;
  expires_at: number;
  resolved_at: number | null;
  cleanup_at: number;
}

export async function insertCryptoDonationInvoice(
  db: D1Database,
  row: Omit<CryptoDonationInvoiceRow,
    "claim_hash" | "status" | "settlement_event_id" | "created_at" | "resolved_at"
  > & { claim_token: string },
): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await db.prepare(
    `INSERT INTO crypto_donation_invoices
      (invoice_id, claim_hash, payment_method, amount_usd_cents,
       amount_atomic, confirmations_required, price_locked_at, status,
       settlement_event_id, created_at, expires_at, resolved_at, cleanup_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?, ?, NULL, ?)`,
  ).bind(
    row.invoice_id,
    await sha256Hex(row.claim_token),
    row.payment_method,
    row.amount_usd_cents,
    row.amount_atomic,
    row.confirmations_required,
    row.price_locked_at,
    now,
    row.expires_at,
    row.cleanup_at,
  ).run();
}

export async function getCryptoDonationInvoice(
  db: D1Database,
  invoiceId: string,
): Promise<CryptoDonationInvoiceRow | null> {
  return await db.prepare("SELECT * FROM crypto_donation_invoices WHERE invoice_id = ?")
    .bind(invoiceId).first<CryptoDonationInvoiceRow>();
}

export async function cryptoDonationClaimMatches(
  row: CryptoDonationInvoiceRow,
  claimToken: string,
): Promise<boolean> {
  const actual = await sha256Hex(claimToken);
  const encoder = new TextEncoder();
  const [actualDigest, storedDigest] = await Promise.all([
    crypto.subtle.digest("SHA-256", encoder.encode(actual)),
    crypto.subtle.digest("SHA-256", encoder.encode(row.claim_hash)),
  ]);
  return crypto.subtle.timingSafeEqual(actualDigest, storedDigest);
}
