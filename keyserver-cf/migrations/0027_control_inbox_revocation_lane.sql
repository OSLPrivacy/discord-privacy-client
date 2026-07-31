-- 0027: a non-evictable, collapsible priority lane for bilateral-burn
-- revocation notices.
--
-- DEPLOYED. Confirmed 2026-07-31 against production: `SELECT name FROM
-- d1_migrations` on osl-keyserver-prod lists 0027_control_inbox_revocation_lane.
-- (This header previously read "NOT DEPLOYED" and stayed stale after the apply,
-- which led a QA lane to conclude the revocation lane was unavailable server-side.
-- It is available. What is still missing is the CLIENT: see below.)
--
-- CLIENT-SIDE GAP, still open as of 2026-07-31: the schema exists but nothing
-- uses it. `apply_peer_revocation` (apps/osl-hub/src/security.rs:2124) and
-- `record_revocation_ack` (:2421) have NO production callers, and the drain path
-- never tests `is_revocation_bundle`. So a bilateral burn issued by A is not
-- applied on B. Deploying this migration did not make the lane live.
--
-- WHY THIS EXISTS
--
-- `evictOldestPending` in src/endpoints/control-inbox.ts silently DELETEs the
-- oldest undelivered rows once a lane reaches its cap, so a new POST always
-- fits. For ordinary content that trade is defensible and deliberate: refusing
-- would punish the sender for something only the recipient can fix, and a
-- recipient who never runs OSL would block the sender for the full seven-day
-- TTL.
--
-- For a burn it is a silent correctness failure. The per-pair cap is 32, so a
-- revocation queued to an offline peer is destroyed by the sender's own next 32
-- messages to that same peer -- with no error, no log, and no notice to either
-- side. The sender's UI would show a burn that was sent; the row it was carried
-- in no longer exists.
--
-- So revocation rows are a separate lane with three properties:
--
--   1. NEVER EVICTED. The Worker does not call evictOldestPending for them, and
--      the ordinary quota triggers below no longer count them, so an ordinary
--      send can neither evict a revocation nor be blocked by one.
--   2. REFUSED, NOT DROPPED, WHEN FULL. The lane has its own caps and the POST
--      answers 507 when they are reached. The sender learns, and retries from
--      its durable outbox. Silence is the one outcome a burn must never have.
--   3. COLLAPSIBLE. A second burn for the same (scope, epoch) UPSERTs onto the
--      first rather than appending, so retrying a burn cannot fill the lane with
--      near-duplicates. The collapse key is an opaque client-computed MAC; this
--      server never learns the scope or the epoch.
--
-- `kind` is a signed component of the POST canonical bytes (the previously
-- reserved empty length-prefixed slot in canonicalControlInboxPostBytes), so an
-- attacker can neither strip `revocation` to make a row evictable nor add it to
-- jump the lane. An old client omits it and produces byte-identical bytes.

-- Empty string, not NULL, so every existing row lands in the ordinary lane and
-- the quota predicates below need no COALESCE.
ALTER TABLE control_inbox ADD COLUMN kind TEXT NOT NULL DEFAULT '';

-- Opaque 64-hex collapse key. NULL for ordinary rows.
ALTER TABLE control_inbox ADD COLUMN collapse_key TEXT;

-- The upsert target. Partial, so the ordinary lane (collapse_key IS NULL) is
-- untouched and can still hold many rows per (recipient, sender, scope).
CREATE UNIQUE INDEX IF NOT EXISTS idx_control_inbox_collapse
  ON control_inbox (recipient_id, sender_id, scope_id, collapse_key)
  WHERE collapse_key IS NOT NULL;

-- Lane counting for both the Worker pre-check and the triggers below.
CREATE INDEX IF NOT EXISTS idx_control_inbox_kind_expiry
  ON control_inbox (recipient_id, kind, expires_at);
CREATE INDEX IF NOT EXISTS idx_control_inbox_kind_sender_expiry
  ON control_inbox (recipient_id, sender_id, kind, expires_at);

-- Re-scope the two existing quota triggers to the ordinary lane only.
--
-- Without this, revocation rows would consume the ordinary lane's budget: 8
-- queued burns from one peer would shrink that peer's content allowance from 32
-- to 24, and once the combined count hit the cap the trigger would ABORT
-- ordinary sends that the Worker's eviction had already made room for. The two
-- lanes must be independent in both directions.
DROP TRIGGER IF EXISTS control_inbox_recipient_quota;
CREATE TRIGGER control_inbox_recipient_quota
BEFORE INSERT ON control_inbox
WHEN NEW.kind = '' AND (
  SELECT COUNT(*)
    FROM control_inbox
   WHERE recipient_id = NEW.recipient_id
     AND kind = ''
     AND expires_at >= unixepoch()
) >= 512
BEGIN
  SELECT RAISE(ABORT, 'control inbox recipient quota exceeded');
END;

DROP TRIGGER IF EXISTS control_inbox_sender_recipient_quota;
CREATE TRIGGER control_inbox_sender_recipient_quota
BEFORE INSERT ON control_inbox
WHEN NEW.kind = '' AND (
  SELECT COUNT(*)
    FROM control_inbox
   WHERE recipient_id = NEW.recipient_id
     AND sender_id = NEW.sender_id
     AND kind = ''
     AND expires_at >= unixepoch()
) >= 32
BEGIN
  SELECT RAISE(ABORT, 'control inbox sender-recipient quota exceeded');
END;

-- The revocation lane's own caps, as the race-safe backstop for the Worker's
-- pre-checks. Deliberately small: a burn is one message per conversation per
-- epoch, and the lane collapses retries, so eight outstanding burns from one
-- peer is already generous. Reaching either cap is a 507, never an eviction.
--
-- Storage stays bounded: 64 rows x 16 KiB = 1 MiB per recipient, on top of the
-- ordinary lane's existing 8 MiB.
-- The `NOT EXISTS` clause is not an optimisation. A BEFORE INSERT trigger fires
-- even when the statement's ON CONFLICT clause is about to turn the insert into
-- an UPDATE, so without it a *retry* of an already-queued burn would abort as
-- soon as the lane was full -- refusing the one write that needs no headroom and
-- that is a client's normal way of making progress. Skipping the count when a
-- collapse target already exists keeps the cap on genuinely new rows only.
CREATE TRIGGER IF NOT EXISTS control_inbox_revocation_recipient_quota
BEFORE INSERT ON control_inbox
WHEN NEW.kind = 'revocation'
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
     AND expires_at >= unixepoch()
) >= 64
BEGIN
  SELECT RAISE(ABORT, 'control inbox revocation lane quota exceeded');
END;

CREATE TRIGGER IF NOT EXISTS control_inbox_revocation_sender_quota
BEFORE INSERT ON control_inbox
WHEN NEW.kind = 'revocation'
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
     AND expires_at >= unixepoch()
) >= 8
BEGIN
  SELECT RAISE(ABORT, 'control inbox revocation sender lane quota exceeded');
END;

-- Only `revocation` and the ordinary empty lane exist. A row with any other
-- kind could only come from a future Worker, and this database should refuse to
-- hold something its quota rules do not cover.
CREATE TRIGGER IF NOT EXISTS control_inbox_kind_is_known
BEFORE INSERT ON control_inbox
WHEN NEW.kind NOT IN ('', 'revocation')
BEGIN
  SELECT RAISE(ABORT, 'control inbox kind is not recognised');
END;

-- A revocation row must carry a collapse key, and an ordinary row must not:
-- otherwise a caller could take an ordinary row out of the evictable set (by
-- attaching a collapse key) or put a revocation into it.
CREATE TRIGGER IF NOT EXISTS control_inbox_collapse_key_matches_kind
BEFORE INSERT ON control_inbox
WHEN (NEW.kind = 'revocation' AND NEW.collapse_key IS NULL)
  OR (NEW.kind = '' AND NEW.collapse_key IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'control inbox collapse key does not match kind');
END;
