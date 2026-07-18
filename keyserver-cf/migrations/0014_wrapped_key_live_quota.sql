-- Enforce wrapped-key storage limits at the D1 write boundary. Only live rows
-- count, so an expired row awaiting the hourly sweep cannot lock a sender out.

CREATE INDEX IF NOT EXISTS idx_wrapped_keys_sender_expiry
  ON wrapped_keys (sender_id, expires_at);

CREATE TRIGGER wrapped_keys_valid_expiry
BEFORE INSERT ON wrapped_keys
WHEN unixepoch(NEW.expires_at) IS NULL
BEGIN
  SELECT RAISE(ABORT, 'wrapped key expiry must be SQLite-parseable');
END;

CREATE TRIGGER wrapped_keys_sender_quota
BEFORE INSERT ON wrapped_keys
WHEN (
  (SELECT COUNT(*)
     FROM wrapped_keys
    WHERE sender_id = NEW.sender_id
      AND unixepoch(expires_at) > unixepoch()) >= 2048
  OR
  (SELECT COALESCE(
      SUM(
        (length(wrapped_share_blob) * 3 / 4) -
        CASE
          WHEN substr(wrapped_share_blob, -2) = '==' THEN 2
          WHEN substr(wrapped_share_blob, -1) = '=' THEN 1
          ELSE 0
        END
      ),
      0
    )
     FROM wrapped_keys
    WHERE sender_id = NEW.sender_id
      AND unixepoch(expires_at) > unixepoch()) +
    (
      (length(NEW.wrapped_share_blob) * 3 / 4) -
      CASE
        WHEN substr(NEW.wrapped_share_blob, -2) = '==' THEN 2
        WHEN substr(NEW.wrapped_share_blob, -1) = '=' THEN 1
        ELSE 0
      END
    ) > 33554432
)
BEGIN
  SELECT RAISE(ABORT, 'wrapped key sender quota exceeded');
END;
