-- Replay receipts for destructive prekey-bundle reads.
--
-- The request digest is SHA-256 over the canonical bytes verified by
-- the requester's registered Ed25519 identity. A primary-key collision
-- aborts the D1 batch before an OPK can be deleted, so an exact signed
-- replay can consume at most one OPK even when requests race.

CREATE TABLE consuming_get_receipts (
  requester_id   TEXT    NOT NULL,
  request_digest BLOB    NOT NULL,
  recipient_id   TEXT    NOT NULL,
  target_id      TEXT    NOT NULL,
  expires_at     INTEGER NOT NULL,
  PRIMARY KEY (requester_id, request_digest)
) WITHOUT ROWID;

CREATE INDEX idx_consuming_get_receipts_expires_at
  ON consuming_get_receipts(expires_at);
