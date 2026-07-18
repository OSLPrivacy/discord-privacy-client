-- A single on-chain payment reference may satisfy at most one OSL invoice.
-- The value is already a watcher-produced SHA-256 commitment, so retaining it
-- establishes replay safety without retaining a transaction id or address.

CREATE TABLE crypto_payment_references_v2 (
  payment_method TEXT NOT NULL CHECK (payment_method IN ('btc', 'xmr')),
  payment_reference_commitment TEXT NOT NULL CHECK (
    length(payment_reference_commitment) = 64
  ),
  invoice_id TEXT NOT NULL UNIQUE,
  event_id TEXT NOT NULL UNIQUE,
  claimed_at INTEGER NOT NULL,
  PRIMARY KEY (payment_method, payment_reference_commitment)
);
