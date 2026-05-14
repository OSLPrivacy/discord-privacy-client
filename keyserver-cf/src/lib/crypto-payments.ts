/// D1 query helpers for the crypto-payments table. The manual
/// review flow lives in src/endpoints/crypto-admin.ts.

export type CryptoStatus =
  | "quoted"
  | "awaiting"
  | "confirmed"
  | "expired"
  | "manually_resolved";

export interface CryptoPaymentRow {
  payment_id: string;
  payment_method: "btc" | "xmr";
  plan: "monthly" | "yearly";
  amount_usd_cents: number;
  amount_native: string;
  address: string;
  customer_email: string;
  status: CryptoStatus;
  txid: string | null;
  created_at: number;
  resolved_at: number | null;
}

export async function insertCryptoQuote(
  db: D1Database,
  row: Omit<CryptoPaymentRow, "status" | "txid" | "created_at" | "resolved_at">,
): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await db
    .prepare(
      `INSERT INTO crypto_pending_payments
         (payment_id, payment_method, plan, amount_usd_cents, amount_native,
          address, customer_email, status, txid, created_at, resolved_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, 'quoted', NULL, ?, NULL)`,
    )
    .bind(
      row.payment_id,
      row.payment_method,
      row.plan,
      row.amount_usd_cents,
      row.amount_native,
      row.address,
      row.customer_email,
      now,
    )
    .run();
}

export async function getCryptoPayment(
  db: D1Database,
  paymentId: string,
): Promise<CryptoPaymentRow | null> {
  return await db
    .prepare("SELECT * FROM crypto_pending_payments WHERE payment_id = ?")
    .bind(paymentId)
    .first<CryptoPaymentRow>();
}

export async function submitCryptoTxid(
  db: D1Database,
  paymentId: string,
  txid: string,
): Promise<boolean> {
  const res = await db
    .prepare(
      `UPDATE crypto_pending_payments
          SET status = 'awaiting', txid = ?
        WHERE payment_id = ? AND status = 'quoted'`,
    )
    .bind(txid, paymentId)
    .run();
  return (res.meta?.changes ?? 0) > 0;
}

export async function confirmCryptoPayment(
  db: D1Database,
  paymentId: string,
): Promise<boolean> {
  const now = Math.floor(Date.now() / 1000);
  const res = await db
    .prepare(
      `UPDATE crypto_pending_payments
          SET status = 'confirmed', resolved_at = ?
        WHERE payment_id = ? AND status = 'awaiting'`,
    )
    .bind(now, paymentId)
    .run();
  return (res.meta?.changes ?? 0) > 0;
}
