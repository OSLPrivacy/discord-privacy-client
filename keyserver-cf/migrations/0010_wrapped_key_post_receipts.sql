-- Short-lived replay receipts for identity-signed wrapped-key uploads.
--
-- A signed request remains fresh for five minutes. Keeping its digest for ten
-- minutes prevents a captured request from resurrecting a row after a burn or
-- single-use consume, including concurrent replay races.

CREATE TABLE wrapped_key_post_receipts (
  sender_id      TEXT    NOT NULL,
  request_digest BLOB    NOT NULL,
  content_id     TEXT    NOT NULL,
  expires_at     INTEGER NOT NULL,
  PRIMARY KEY (sender_id, request_digest)
) WITHOUT ROWID;

CREATE INDEX idx_wrapped_key_post_receipts_expires_at
  ON wrapped_key_post_receipts(expires_at);
