-- Privacy-minimal audit trail for refused watcher settlement callbacks.
-- The record keeps the refusal reason and a one-way invoice id hash, never an
-- address, transaction id, claim token, delivery key, account, email, or IP.

CREATE TABLE crypto_settlement_refusals (
  refusal_id TEXT PRIMARY KEY CHECK (
    length(refusal_id) = 68
    AND substr(refusal_id, 1, 4) = 'csr_'
    AND substr(refusal_id, 5) NOT GLOB '*[^0-9a-f]*'
  ),
  invoice_id_hash TEXT NOT NULL CHECK (
    length(invoice_id_hash) = 64
    AND invoice_id_hash NOT GLOB '*[^0-9a-f]*'
  ),
  event_id TEXT NOT NULL,
  payment_method TEXT NOT NULL CHECK (payment_method IN ('btc', 'xmr')),
  reason TEXT NOT NULL CHECK (reason IN ('bad-signature')),
  attempted_at INTEGER NOT NULL
);

CREATE INDEX idx_crypto_settlement_refusals_invoice_reason
  ON crypto_settlement_refusals (invoice_id_hash, reason, attempted_at);
