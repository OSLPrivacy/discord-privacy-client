-- Recipient-authenticated opened receipts for single-use wrapped keys.
--
-- The Worker inserts one receipt and deletes the corresponding single-use
-- wrapped key in the same D1 batch. The receipt gives later fresh claims a
-- durable "already opened" answer instead of turning the second claim into an
-- indistinguishable unknown-content miss.

CREATE TABLE wrapped_key_open_receipts (
  recipient_id TEXT NOT NULL,
  signer_ed25519_pub TEXT NOT NULL,
  request_digest BLOB NOT NULL,
  content_id TEXT NOT NULL,
  opened_at TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  PRIMARY KEY (recipient_id, request_digest)
) WITHOUT ROWID;

CREATE INDEX idx_wrapped_key_open_receipts_content
  ON wrapped_key_open_receipts(content_id, recipient_id);

CREATE INDEX idx_wrapped_key_open_receipts_expires
  ON wrapped_key_open_receipts(expires_at);
