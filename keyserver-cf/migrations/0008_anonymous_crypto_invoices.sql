-- Anonymous, node-verified crypto invoices. This intentionally does not
-- retain an email address, submitted transaction id, IP address, service
-- account, or other customer identifier. The self-hosted watcher owns the
-- short-lived address-to-invoice mapping and reports only an opaque event.

CREATE TABLE crypto_invoices_v2 (
  invoice_id TEXT PRIMARY KEY,
  claim_hash TEXT NOT NULL UNIQUE,
  payment_method TEXT NOT NULL CHECK (payment_method IN ('btc', 'xmr')),
  plan TEXT NOT NULL CHECK (plan IN ('monthly', 'yearly')),
  amount_usd_cents INTEGER NOT NULL CHECK (amount_usd_cents > 0),
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

CREATE INDEX idx_crypto_invoices_v2_cleanup
  ON crypto_invoices_v2 (cleanup_at);

CREATE TABLE crypto_settlement_events_v2 (
  event_id TEXT PRIMARY KEY,
  invoice_id TEXT NOT NULL,
  processed_at INTEGER NOT NULL
);

CREATE INDEX idx_crypto_settlement_events_v2_processed
  ON crypto_settlement_events_v2 (processed_at);
