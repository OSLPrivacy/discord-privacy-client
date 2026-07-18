-- Bound targeted opaque-storage abuse even when an attacker rotates through
-- many self-registered identities. These are admission limits, not billing
-- authority; all payment entitlements remain server-issued and signed.

CREATE INDEX IF NOT EXISTS idx_control_inbox_recipient_sender_expiry
  ON control_inbox (recipient_id, sender_id, expires_at);

CREATE TRIGGER control_inbox_sender_recipient_quota
BEFORE INSERT ON control_inbox
WHEN (
  SELECT COUNT(*)
    FROM control_inbox
   WHERE recipient_id = NEW.recipient_id
     AND sender_id = NEW.sender_id
     AND expires_at >= unixepoch()
) >= 32
BEGIN
  SELECT RAISE(ABORT, 'control inbox sender-recipient quota exceeded');
END;

CREATE INDEX IF NOT EXISTS idx_wrapped_keys_recipient_expiry
  ON wrapped_keys (recipient_id, expires_at);

CREATE TRIGGER wrapped_keys_recipient_quota
BEFORE INSERT ON wrapped_keys
WHEN (
  (SELECT COUNT(*)
     FROM wrapped_keys
    WHERE recipient_id = NEW.recipient_id
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
    WHERE recipient_id = NEW.recipient_id
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
  SELECT RAISE(ABORT, 'wrapped key recipient quota exceeded');
END;
