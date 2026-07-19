-- Crypto invoice rows and settlement replay rows are intentionally short-lived.
-- Preserve only the aggregate-safe paid fact needed for lifetime commerce
-- reporting. No address, transaction reference, claim capability, key, email,
-- account, or other customer detail is retained here.

CREATE TABLE crypto_commerce_events (
  subscription_id TEXT PRIMARY KEY,
  payment_method TEXT CHECK (payment_method IS NULL OR payment_method IN ('btc', 'xmr')),
  amount_usd_cents INTEGER NOT NULL CHECK (amount_usd_cents > 0),
  settled_at INTEGER NOT NULL
);

CREATE INDEX idx_crypto_commerce_events_settled
  ON crypto_commerce_events (settled_at);

-- Recover durable metrics for any crypto entitlement created before this
-- migration. The invoice join preserves the asset when its short-lived row is
-- still present; already-tombstoned invoices remain countable without it.
INSERT OR IGNORE INTO crypto_commerce_events (
  subscription_id, payment_method, amount_usd_cents, settled_at
)
SELECT
  subscriptions.subscription_id,
  crypto_invoices_v2.payment_method,
  500,
  COALESCE(crypto_invoices_v2.resolved_at, subscriptions.created_at)
FROM subscriptions
LEFT JOIN crypto_invoices_v2
  ON subscriptions.subscription_id = 'crypto_' || crypto_invoices_v2.invoice_id
WHERE length(subscriptions.subscription_id) = 44
  AND substr(subscriptions.subscription_id, 1, 12) = 'crypto_cpay_'
  AND substr(subscriptions.subscription_id, 13) NOT GLOB '*[^0-9a-f]*'
  AND subscriptions.customer_id = 'crypto:' || substr(subscriptions.subscription_id, 8)
  AND subscriptions.is_comp = 0;
