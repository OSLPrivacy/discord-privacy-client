-- F1.2 schema: Stripe subscriptions + license issuance + webhook
-- idempotency. Adds three tables on top of the F1.1 baseline.
--
-- Design decisions (locked in F1 discovery):
--   - License plaintext is NEVER stored. Only SHA-256(plaintext).
--     The plaintext appears once in the issuance email; client
--     stores it locally sealed.
--   - Webhook idempotency uses Stripe's event.id as the dedup key.
--     INSERT OR IGNORE catches automatic retries (up to 3 days).
--   - 6 subscription states: PENDING, ACTIVE, CANCELLED, GRACE,
--     REVOKED, EXPIRED. Stored as a TEXT column for readability /
--     observability over the wire.

CREATE TABLE subscriptions (
  subscription_id TEXT PRIMARY KEY,        -- Stripe sub_xxx
  customer_id TEXT NOT NULL,               -- Stripe cus_xxx
  customer_email TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN (
    'PENDING', 'ACTIVE', 'CANCELLED', 'GRACE', 'REVOKED', 'EXPIRED'
  )),
  current_period_end INTEGER,              -- unix seconds; NULL while PENDING
  cancel_at_period_end INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX idx_subs_customer ON subscriptions (customer_id);
CREATE INDEX idx_subs_email ON subscriptions (customer_email);
CREATE INDEX idx_subs_status_period ON subscriptions (status, current_period_end);

CREATE TABLE licenses (
  license_hash TEXT PRIMARY KEY,           -- SHA-256(plaintext) as lowercase hex
  subscription_id TEXT NOT NULL,
  issued_at INTEGER NOT NULL,
  revoked_at INTEGER,
  revoked_reason TEXT CHECK (
    revoked_reason IS NULL OR
    revoked_reason IN ('chargeback', 'fraud', 'manual')
  ),
  FOREIGN KEY (subscription_id) REFERENCES subscriptions (subscription_id)
);
CREATE INDEX idx_licenses_sub ON licenses (subscription_id);

-- Webhook dedup. Stripe retries identical events with the same
-- event.id; first writer wins via PK collision on INSERT OR IGNORE.
CREATE TABLE stripe_events (
  event_id TEXT PRIMARY KEY,               -- evt_xxx
  event_type TEXT NOT NULL,
  processed_at INTEGER NOT NULL
);
