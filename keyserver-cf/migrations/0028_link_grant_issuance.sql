-- 0028: bookkeeping for view-once link-creation grants.
--
-- NOT DEPLOYED. This Worker serves the owner's real identity; applying it is
-- their decision. See keyserver-cf/DEPLOY.md.
--
-- WHAT THIS DOES NOT STORE
--
-- Neither table holds a link id, a link URL, a capability token, a ciphertext,
-- a recipient, an IP or a user agent. The keyserver never learns that a link
-- exists, let alone what is in it -- it only knows that an identity asked to be
-- vouched for. That separation is the privacy property of the whole lane: the
-- keyserver knows who asked and never sees the link; the cipher-store sees the
-- link and never knows who asked.
--
-- The one thing these rows do reveal, to an operator with database access, is
-- how many grants an identity requested today. That is the unavoidable cost of
-- having a durable per-identity cap at all, and it is deliberately the cheapest
-- form of it: a bucket and a count, no timestamps per issuance, no history
-- beyond the current day, swept by cron.

-- Single-use anchor for the SIGNED issuance request.
--
-- request_digest is SHA-256 over the canonical bytes the identity signed
-- (canonicalLinkGrantBytes). A primary-key collision aborts the batch before
-- the quota row moves, so a captured request replayed inside the freshness
-- window yields 409 rather than a second grant.
CREATE TABLE link_grant_receipts (
  user_id        TEXT    NOT NULL,
  request_digest BLOB    NOT NULL,
  expires_at     INTEGER NOT NULL,
  PRIMARY KEY (user_id, request_digest)
) WITHOUT ROWID;

CREATE INDEX idx_link_grant_receipts_expires_at
  ON link_grant_receipts(expires_at);

-- Durable per-identity daily ceiling. `day` is floor(unix_seconds / 86400),
-- i.e. a UTC day bucket; yesterday's row is swept, so the table stays roughly
-- one row per active identity.
CREATE TABLE link_grant_quota (
  user_id TEXT    NOT NULL,
  day     INTEGER NOT NULL,
  issued  INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, day)
) WITHOUT ROWID;

CREATE INDEX idx_link_grant_quota_day ON link_grant_quota(day);

-- The cap, enforced at the write boundary rather than by a read-then-write in
-- the Worker.
--
-- A BEFORE INSERT trigger fires even when the statement's ON CONFLICT clause is
-- about to turn the insert into an UPDATE (the same property migration 0027
-- documents and relies on). That is what makes this race-safe: two concurrent
-- issuance requests from one identity at the ceiling cannot both read 49 and
-- both write 50.
--
-- 50/day is far above any plausible human use of a lane that exists for
-- recipients who do not have OSL, and far below what would make the store
-- attractive as a file host. It bounds one identity; the anti-Sybil bound is
-- the per-IP limit on issuance and on /v1/register.
CREATE TRIGGER link_grant_daily_quota
BEFORE INSERT ON link_grant_quota
WHEN (
  SELECT issued FROM link_grant_quota
   WHERE user_id = NEW.user_id AND day = NEW.day
) >= 50
BEGIN
  SELECT RAISE(ABORT, 'link grant daily quota exceeded');
END;
