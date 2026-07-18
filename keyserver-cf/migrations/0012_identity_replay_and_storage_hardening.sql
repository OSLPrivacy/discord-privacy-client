-- Fresh signed mutations are single-use across Worker isolates. Receipts live
-- for twice the five-minute signature window; only digests are retained.

CREATE TABLE prekey_replenish_receipts (
  user_id               TEXT NOT NULL,
  signer_ed25519_pub     TEXT NOT NULL,
  request_digest         BLOB NOT NULL,
  expires_at             INTEGER NOT NULL,
  PRIMARY KEY (user_id, signer_ed25519_pub, request_digest)
) WITHOUT ROWID;

CREATE INDEX idx_prekey_replenish_receipts_expiry
  ON prekey_replenish_receipts (expires_at);

CREATE TABLE wrapped_key_burn_receipts (
  user_id               TEXT NOT NULL,
  signer_ed25519_pub     TEXT NOT NULL,
  request_digest         BLOB NOT NULL,
  expires_at             INTEGER NOT NULL,
  PRIMARY KEY (user_id, signer_ed25519_pub, request_digest)
) WITHOUT ROWID;

CREATE INDEX idx_wrapped_key_burn_receipts_expiry
  ON wrapped_key_burn_receipts (expires_at);

CREATE TABLE unregister_receipts (
  user_id               TEXT NOT NULL,
  signer_ed25519_pub     TEXT NOT NULL,
  request_digest         BLOB NOT NULL,
  expires_at             INTEGER NOT NULL,
  PRIMARY KEY (user_id, signer_ed25519_pub, request_digest)
) WITHOUT ROWID;

CREATE INDEX idx_unregister_receipts_expiry
  ON unregister_receipts (expires_at);
