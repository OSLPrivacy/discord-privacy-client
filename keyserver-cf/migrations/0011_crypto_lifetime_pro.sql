-- Crypto checkout is now one explicit $5 one-time lifetime Pro purchase.
-- Legacy recurring crypto invoices were never live and are deliberately not
-- promoted into lifetime entitlements by this migration.

CREATE TABLE crypto_invoices_v2_lifetime (
  invoice_id TEXT PRIMARY KEY,
  claim_hash TEXT NOT NULL UNIQUE,
  payment_method TEXT NOT NULL CHECK (payment_method IN ('btc', 'xmr')),
  plan TEXT NOT NULL CHECK (plan = 'pro'),
  amount_usd_cents INTEGER NOT NULL CHECK (amount_usd_cents = 500),
  amount_atomic TEXT NOT NULL,
  delivery_public_key_spki TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN (
    'pending', 'paid', 'expired', 'delivery_ready'
  )),
  settlement_event_id TEXT UNIQUE,
  encrypted_license TEXT,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  resolved_at INTEGER,
  cleanup_at INTEGER NOT NULL
);

DROP TABLE crypto_invoices_v2;
ALTER TABLE crypto_invoices_v2_lifetime RENAME TO crypto_invoices_v2;

CREATE INDEX idx_crypto_invoices_v2_cleanup
  ON crypto_invoices_v2 (cleanup_at);
