-- Two-phase, browser-bound entitlement delivery and owner-approved comp
-- batches. Plaintext activation codes are never persisted.

ALTER TABLE stripe_checkout_claims ADD COLUMN acknowledged_at INTEGER;
ALTER TABLE crypto_invoices_v2 ADD COLUMN delivered_at INTEGER;
ALTER TABLE crypto_invoices_v2 ADD COLUMN acknowledged_at INTEGER;

CREATE TABLE comp_batches (
  batch_id TEXT PRIMARY KEY,
  request_hash TEXT NOT NULL UNIQUE,
  issuer TEXT NOT NULL CHECK (issuer IN ('production', 'qa')),
  purpose_hash TEXT NOT NULL,
  quantity INTEGER NOT NULL CHECK (quantity BETWEEN 1 AND 25),
  expires_at INTEGER NOT NULL,
  issued_at INTEGER NOT NULL,
  revoked_at INTEGER,
  audit_digest TEXT NOT NULL UNIQUE
);

CREATE INDEX idx_comp_batches_expiry ON comp_batches (expires_at, revoked_at);

CREATE TABLE comp_batch_licenses (
  batch_id TEXT NOT NULL,
  license_hash TEXT NOT NULL UNIQUE,
  subscription_id TEXT NOT NULL UNIQUE,
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 25),
  PRIMARY KEY (batch_id, ordinal),
  FOREIGN KEY (batch_id) REFERENCES comp_batches(batch_id),
  FOREIGN KEY (license_hash) REFERENCES licenses(license_hash),
  FOREIGN KEY (subscription_id) REFERENCES subscriptions(subscription_id)
);

CREATE INDEX idx_comp_batch_licenses_batch ON comp_batch_licenses (batch_id);
