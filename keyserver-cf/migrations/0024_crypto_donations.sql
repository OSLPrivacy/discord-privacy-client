-- Separate, privacy-minimal Bitcoin and Monero donation invoices. These rows
-- are transient capabilities and never carry entitlement, account, address,
-- transaction, delivery-key, or donor identity data.

CREATE TABLE crypto_donation_invoices (
  invoice_id TEXT PRIMARY KEY CHECK (
    length(invoice_id) = 37
    AND substr(invoice_id, 1, 5) = 'cdon_'
    AND substr(invoice_id, 6) NOT GLOB '*[^0-9a-f]*'
  ),
  claim_hash TEXT NOT NULL UNIQUE CHECK (length(claim_hash) = 64),
  payment_method TEXT NOT NULL CHECK (payment_method IN ('btc', 'xmr')),
  amount_usd_cents INTEGER NOT NULL CHECK (
    typeof(amount_usd_cents) = 'integer'
    AND amount_usd_cents >= 100
    AND amount_usd_cents <= 1000000
  ),
  amount_atomic TEXT NOT NULL CHECK (
    length(amount_atomic) BETWEEN 1 AND 31
    AND amount_atomic NOT GLOB '*[^0-9]*'
    AND substr(amount_atomic, 1, 1) != '0'
  ),
  confirmations_required INTEGER NOT NULL CHECK (confirmations_required > 0),
  price_locked_at INTEGER NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending', 'paid', 'recorded', 'expired')),
  settlement_event_id TEXT UNIQUE,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  resolved_at INTEGER,
  cleanup_at INTEGER NOT NULL
);

CREATE INDEX idx_crypto_donation_invoices_cleanup
  ON crypto_donation_invoices (cleanup_at);

-- Durable aggregate-safe donation facts. donation_id is opaque and carries no
-- chain reference, invoice capability, address, message, or donor identity.
CREATE TABLE crypto_donation_events (
  donation_id TEXT PRIMARY KEY CHECK (
    length(donation_id) = 44
    AND substr(donation_id, 1, 12) = 'crypto_cdon_'
    AND substr(donation_id, 13) NOT GLOB '*[^0-9a-f]*'
  ),
  payment_method TEXT NOT NULL CHECK (payment_method IN ('btc', 'xmr')),
  amount_usd_cents INTEGER NOT NULL CHECK (
    typeof(amount_usd_cents) = 'integer'
    AND amount_usd_cents >= 100
    AND amount_usd_cents <= 1000000
  ),
  settled_at INTEGER NOT NULL
);

CREATE INDEX idx_crypto_donation_events_settled
  ON crypto_donation_events (settled_at);
