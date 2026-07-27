-- 0031: retain control-inbox payloads whose sender cannot currently be looked
-- up after migration 0029.
--
-- Migration 0029 deliberately disabled every pre-existing identity until the
-- owner re-registers with the current signing key. Rows already queued by one
-- of those identities remain authenticated opaque bytes, but a new client
-- cannot verify them while lookup is disabled. Deleting those rows solely
-- because their original TTL elapsed turns a reversible identity transition
-- into silent message loss.
--
-- This migration is additive and old-Worker compatible. Existing rows and
-- legacy INSERT statements take the exact live/empty metadata defaults. The
-- status-aware Worker MUST be deployed after this migration, because it names
-- these columns. Once that Worker has classified any row, do not roll back to
-- a pre-0031 Worker: an old drain ignores delivery_status and would return
-- quarantined or retired bytes as though they were live.

ALTER TABLE control_inbox
  ADD COLUMN delivery_status TEXT NOT NULL DEFAULT 'live'
  CHECK (delivery_status IN ('live', 'retryable', 'quarantined', 'retired'));

ALTER TABLE control_inbox
  ADD COLUMN delivery_reason TEXT
  CHECK (
    delivery_reason IS NULL OR delivery_reason IN (
      'sender_lookup_disabled',
      'sender_lookup_retry_exhausted',
      'sender_identifier_malformed',
      'sender_discord_snowflake'
    )
  );

ALTER TABLE control_inbox
  ADD COLUMN delivery_attempts INTEGER NOT NULL DEFAULT 0
  CHECK (delivery_attempts BETWEEN 0 AND 3);

ALTER TABLE control_inbox
  ADD COLUMN sender_disabled_first_seen_at INTEGER;

ALTER TABLE control_inbox
  ADD COLUMN delivery_next_retry_at INTEGER;

-- A disabled row remains present through at least one full inbox TTL after it
-- is first observed. This is separate from expires_at: the latter is the
-- ordinary delivery TTL, while this column is the bounded quarantine hold.
ALTER TABLE control_inbox
  ADD COLUMN delivery_retain_until INTEGER;

CREATE INDEX idx_control_inbox_delivery_reconcile
  ON control_inbox (delivery_status, delivery_next_retry_at, created_at, id);

CREATE INDEX idx_control_inbox_sender_delivery
  ON control_inbox (
    recipient_id,
    sender_id,
    delivery_status,
    expires_at,
    delivery_retain_until
  );

CREATE INDEX idx_control_inbox_live_kind_expiry
  ON control_inbox (
    recipient_id,
    kind,
    delivery_status,
    expires_at
  );

CREATE INDEX idx_control_inbox_live_sender_kind_expiry
  ON control_inbox (
    recipient_id,
    sender_id,
    kind,
    delivery_status,
    expires_at
  );

-- Retained non-live rows are storage held for recovery, not pending live
-- delivery. They are never selected by the ordinary sender's recycling path,
-- but they DO consume the physical lane cap. Otherwise a disabled sender can
-- accumulate an unbounded second quota behind the live quota. Rebuild the D1
-- race backstops so a new live row is refused once all physical rows in the
-- lane reach the bound.
DROP TRIGGER IF EXISTS control_inbox_recipient_quota;
CREATE TRIGGER control_inbox_recipient_quota
BEFORE INSERT ON control_inbox
WHEN NEW.kind = '' AND NEW.delivery_status = 'live' AND (
  SELECT COUNT(*)
    FROM control_inbox
   WHERE recipient_id = NEW.recipient_id
     AND kind = ''
) >= 512
BEGIN
  SELECT RAISE(ABORT, 'control inbox recipient quota exceeded');
END;

DROP TRIGGER IF EXISTS control_inbox_sender_recipient_quota;
CREATE TRIGGER control_inbox_sender_recipient_quota
BEFORE INSERT ON control_inbox
WHEN NEW.kind = '' AND NEW.delivery_status = 'live' AND (
  SELECT COUNT(*)
    FROM control_inbox
   WHERE recipient_id = NEW.recipient_id
     AND sender_id = NEW.sender_id
     AND kind = ''
) >= 32
BEGIN
  SELECT RAISE(ABORT, 'control inbox sender-recipient quota exceeded');
END;

DROP TRIGGER IF EXISTS control_inbox_revocation_recipient_quota;
CREATE TRIGGER control_inbox_revocation_recipient_quota
BEFORE INSERT ON control_inbox
WHEN NEW.kind = 'revocation'
 AND NEW.delivery_status = 'live'
 AND NOT EXISTS (
   SELECT 1 FROM control_inbox
    WHERE recipient_id = NEW.recipient_id
      AND sender_id = NEW.sender_id
      AND scope_id = NEW.scope_id
      AND collapse_key = NEW.collapse_key
 )
 AND (
  SELECT COUNT(*)
    FROM control_inbox
   WHERE recipient_id = NEW.recipient_id
     AND kind = 'revocation'
) >= 64
BEGIN
  SELECT RAISE(ABORT, 'control inbox revocation lane quota exceeded');
END;

DROP TRIGGER IF EXISTS control_inbox_revocation_sender_quota;
CREATE TRIGGER control_inbox_revocation_sender_quota
BEFORE INSERT ON control_inbox
WHEN NEW.kind = 'revocation'
 AND NEW.delivery_status = 'live'
 AND NOT EXISTS (
   SELECT 1 FROM control_inbox
    WHERE recipient_id = NEW.recipient_id
      AND sender_id = NEW.sender_id
      AND scope_id = NEW.scope_id
      AND collapse_key = NEW.collapse_key
 )
 AND (
  SELECT COUNT(*)
    FROM control_inbox
   WHERE recipient_id = NEW.recipient_id
     AND sender_id = NEW.sender_id
     AND kind = 'revocation'
) >= 8
BEGIN
  SELECT RAISE(ABORT, 'control inbox revocation sender lane quota exceeded');
END;

-- Release gates require this exact marker before the status-aware Worker is
-- considered healthy. A pre-0031 schema has no table and is refused; a rolled
-- back Worker omits the capability from healthz and is therefore detectable.
CREATE TABLE IF NOT EXISTS worker_schema_capabilities (
  capability TEXT PRIMARY KEY,
  version    INTEGER NOT NULL CHECK (version >= 1)
) WITHOUT ROWID;

INSERT INTO worker_schema_capabilities (capability, version)
VALUES ('control_inbox_sender_disposition', 1);

-- Migration-first protection for the brief old-Worker window. The pre-0031
-- scheduled DELETE names only expires_at; this trigger makes that statement a
-- no-op for a sender whose lookup is disabled/missing until the new Worker has
-- recorded a disposition. It also protects every retained non-live payload
-- from an old drain/delete path until the bounded hold has elapsed.
CREATE TRIGGER control_inbox_retention_delete_guard
BEFORE DELETE ON control_inbox
WHEN
  (
    OLD.delivery_status = 'live'
    AND NOT EXISTS (
      SELECT 1 FROM users
       WHERE user_id = OLD.sender_id
         AND identity_lookup_enabled = 1
    )
  )
  OR
  (
    OLD.delivery_status = 'retryable'
  )
  OR
  (
    OLD.delivery_status IN ('quarantined', 'retired')
    AND (
      OLD.delivery_retain_until IS NULL
      OR OLD.delivery_retain_until >= unixepoch()
    )
  )
BEGIN
  SELECT RAISE(IGNORE);
END;

-- Durable transition authority. Shape checks below prevent internally
-- inconsistent metadata; these triggers additionally bind non-live state to
-- lookup truth and prevent arbitrary jumps between valid-looking states.
CREATE TRIGGER control_inbox_delivery_lookup_insert_guard
BEFORE INSERT ON control_inbox
WHEN NEW.delivery_status <> 'live'
 AND NOT (
   NEW.delivery_status = 'retired'
   AND length(NEW.sender_id) BETWEEN 17 AND 20
   AND NEW.sender_id NOT GLOB '*[^0-9]*'
 )
 AND EXISTS (
   SELECT 1 FROM users
    WHERE user_id = NEW.sender_id
      AND identity_lookup_enabled = 1
 )
BEGIN
  SELECT RAISE(ABORT, 'control inbox sender is lookup-enabled');
END;

CREATE TRIGGER control_inbox_delivery_lookup_update_guard
BEFORE UPDATE OF
  delivery_status,
  delivery_reason,
  delivery_attempts,
  sender_disabled_first_seen_at,
  delivery_next_retry_at,
  delivery_retain_until
ON control_inbox
WHEN
  (
    NEW.delivery_status <> 'live'
    AND NOT (
      NEW.delivery_status = 'retired'
      AND length(NEW.sender_id) BETWEEN 17 AND 20
      AND NEW.sender_id NOT GLOB '*[^0-9]*'
    )
    AND EXISTS (
      SELECT 1 FROM users
       WHERE user_id = NEW.sender_id
         AND identity_lookup_enabled = 1
    )
  )
  OR
  (
    OLD.delivery_status <> 'live'
    AND NEW.delivery_status = 'live'
    AND (
      (
        length(NEW.sender_id) BETWEEN 17 AND 20
        AND NEW.sender_id NOT GLOB '*[^0-9]*'
      )
      OR NOT EXISTS (
        SELECT 1 FROM users
         WHERE user_id = NEW.sender_id
           AND identity_lookup_enabled = 1
      )
    )
  )
BEGIN
  SELECT RAISE(ABORT, 'control inbox delivery state contradicts lookup');
END;

CREATE TRIGGER control_inbox_delivery_transition_guard
BEFORE UPDATE OF
  delivery_status,
  delivery_reason,
  delivery_attempts,
  sender_disabled_first_seen_at,
  delivery_next_retry_at,
  delivery_retain_until
ON control_inbox
WHEN NOT (
  (OLD.delivery_status = 'live'
   AND NEW.delivery_status IN ('live', 'retryable', 'retired'))
  OR
  (OLD.delivery_status = 'retryable'
   AND NEW.delivery_status IN (
     'retryable', 'quarantined', 'retired', 'live'
   ))
  OR
  (OLD.delivery_status = 'quarantined'
   AND NEW.delivery_status IN ('retired', 'live'))
)
BEGIN
  SELECT RAISE(ABORT, 'control inbox delivery transition is invalid');
END;

-- Keep the state machine valid even if a future Worker names these columns
-- incorrectly. In particular, a row cannot be hidden from drains merely by
-- flipping delivery_status without recording the reason, first observation,
-- retry count, and bounded retention deadline.
CREATE TRIGGER control_inbox_delivery_state_insert_guard
BEFORE INSERT ON control_inbox
WHEN COALESCE(
  (
    NEW.delivery_status = 'live'
    AND NEW.delivery_reason IS NULL
    AND NEW.delivery_attempts = 0
    AND NEW.sender_disabled_first_seen_at IS NULL
    AND NEW.delivery_next_retry_at IS NULL
    AND NEW.delivery_retain_until IS NULL
  )
  OR
  (
    NEW.delivery_status = 'retryable'
    AND NEW.delivery_reason IN (
      'sender_lookup_disabled',
      'sender_identifier_malformed'
    )
    AND NEW.delivery_attempts BETWEEN 1 AND 2
    AND NEW.sender_disabled_first_seen_at > 0
    AND NEW.delivery_next_retry_at > NEW.sender_disabled_first_seen_at
    AND NEW.delivery_retain_until =
        NEW.sender_disabled_first_seen_at + 604800
    AND NEW.delivery_next_retry_at <= NEW.delivery_retain_until
    AND NOT (
      length(NEW.sender_id) BETWEEN 17 AND 20
      AND NEW.sender_id NOT GLOB '*[^0-9]*'
    )
  )
  OR
  (
    NEW.delivery_status = 'quarantined'
    AND NEW.delivery_reason IN (
      'sender_lookup_retry_exhausted',
      'sender_identifier_malformed'
    )
    AND NEW.delivery_attempts = 3
    AND NEW.sender_disabled_first_seen_at > 0
    AND NEW.delivery_next_retry_at IS NULL
    AND NEW.delivery_retain_until =
        NEW.sender_disabled_first_seen_at + 604800
    AND NOT (
      length(NEW.sender_id) BETWEEN 17 AND 20
      AND NEW.sender_id NOT GLOB '*[^0-9]*'
    )
  )
  OR
  (
    NEW.delivery_status = 'retired'
    AND NEW.delivery_reason = 'sender_discord_snowflake'
    AND NEW.delivery_attempts = 0
    AND NEW.sender_disabled_first_seen_at > 0
    AND NEW.delivery_next_retry_at IS NULL
    AND NEW.delivery_retain_until =
        NEW.sender_disabled_first_seen_at + 604800
    AND length(NEW.sender_id) BETWEEN 17 AND 20
    AND NEW.sender_id NOT GLOB '*[^0-9]*'
  ),
  0
) = 0
BEGIN
  SELECT RAISE(ABORT, 'control inbox delivery state is inconsistent');
END;

CREATE TRIGGER control_inbox_delivery_state_update_guard
BEFORE UPDATE OF
  delivery_status,
  delivery_reason,
  delivery_attempts,
  sender_disabled_first_seen_at,
  delivery_next_retry_at,
  delivery_retain_until
ON control_inbox
WHEN COALESCE(
  (
    NEW.delivery_status = 'live'
    AND NEW.delivery_reason IS NULL
    AND NEW.delivery_attempts = 0
    AND NEW.sender_disabled_first_seen_at IS NULL
    AND NEW.delivery_next_retry_at IS NULL
    AND NEW.delivery_retain_until IS NULL
  )
  OR
  (
    NEW.delivery_status = 'retryable'
    AND NEW.delivery_reason IN (
      'sender_lookup_disabled',
      'sender_identifier_malformed'
    )
    AND NEW.delivery_attempts BETWEEN 1 AND 2
    AND NEW.sender_disabled_first_seen_at > 0
    AND NEW.delivery_next_retry_at > NEW.sender_disabled_first_seen_at
    AND NEW.delivery_retain_until =
        NEW.sender_disabled_first_seen_at + 604800
    AND NEW.delivery_next_retry_at <= NEW.delivery_retain_until
    AND NOT (
      length(NEW.sender_id) BETWEEN 17 AND 20
      AND NEW.sender_id NOT GLOB '*[^0-9]*'
    )
  )
  OR
  (
    NEW.delivery_status = 'quarantined'
    AND NEW.delivery_reason IN (
      'sender_lookup_retry_exhausted',
      'sender_identifier_malformed'
    )
    AND NEW.delivery_attempts = 3
    AND NEW.sender_disabled_first_seen_at > 0
    AND NEW.delivery_next_retry_at IS NULL
    AND NEW.delivery_retain_until =
        NEW.sender_disabled_first_seen_at + 604800
    AND NOT (
      length(NEW.sender_id) BETWEEN 17 AND 20
      AND NEW.sender_id NOT GLOB '*[^0-9]*'
    )
  )
  OR
  (
    NEW.delivery_status = 'retired'
    AND NEW.delivery_reason = 'sender_discord_snowflake'
    AND NEW.delivery_attempts = 0
    AND NEW.sender_disabled_first_seen_at > 0
    AND NEW.delivery_next_retry_at IS NULL
    AND NEW.delivery_retain_until =
        NEW.sender_disabled_first_seen_at + 604800
    AND length(NEW.sender_id) BETWEEN 17 AND 20
    AND NEW.sender_id NOT GLOB '*[^0-9]*'
  ),
  0
) = 0
BEGIN
  SELECT RAISE(ABORT, 'control inbox delivery state is inconsistent');
END;
