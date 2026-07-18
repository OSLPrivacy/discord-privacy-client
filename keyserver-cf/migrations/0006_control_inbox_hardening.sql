-- Make signed control-inbox POST retries idempotent.
--
-- `request_digest` is SHA-256 over the exact server-reconstructed
-- canonical request bytes that the sender signed. It lives in a
-- separate short-lived receipt table so deleting an applied inbox row
-- cannot make that same signed request enqueue a second time.
--
-- The composite primary key is the final arbiter for simultaneous
-- retries, rather than a racy read-before-write check in the Worker.

CREATE TABLE control_inbox_requests (
  sender_id      TEXT    NOT NULL,
  request_digest BLOB    NOT NULL,
  inbox_id       BLOB    NOT NULL,
  recipient_id   TEXT    NOT NULL,
  expires_at     INTEGER NOT NULL,
  PRIMARY KEY (sender_id, request_digest)
) WITHOUT ROWID;

CREATE INDEX idx_control_inbox_requests_expires_at
  ON control_inbox_requests(expires_at);

-- The Worker performs a friendly pre-check, while this trigger closes
-- the concurrent-writer race at the database boundary. 512 maximum-size
-- bundles cap live opaque storage for one recipient at 8 MiB.
CREATE TRIGGER control_inbox_recipient_quota
BEFORE INSERT ON control_inbox
WHEN (
  SELECT COUNT(*)
    FROM control_inbox
   WHERE recipient_id = NEW.recipient_id
     AND expires_at >= unixepoch()
) >= 512
BEGIN
  SELECT RAISE(ABORT, 'control inbox recipient quota exceeded');
END;
