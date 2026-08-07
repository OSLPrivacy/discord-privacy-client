-- One-use invite links.
--
-- 0042 remains deliberately skipped by the migration-sequence gate because a
-- separate local lane already used that number. This migration takes the next
-- ordinary keyserver number and defines only the durable record shape needed by
-- invite issuance/acceptance: who created the invite, the single purpose it is
-- meant to serve, when it expires, and whether it has already been consumed.
--
-- The bearer invite secret itself is not stored here. `invite_id` is an opaque
-- random lookup key/capability digest owned by the caller-facing invite flow.
CREATE TABLE one_use_invite_links (
  invite_id    BLOB    PRIMARY KEY NOT NULL CHECK(length(invite_id) = 16),
  creator      BLOB    NOT NULL CHECK(length(creator) = 32),
  intended_use TEXT    NOT NULL CHECK(intended_use = 'space_admission'),
  use_limit    INTEGER NOT NULL DEFAULT 1 CHECK(use_limit = 1),
  created_at   INTEGER NOT NULL,
  expires_at   INTEGER NOT NULL CHECK(expires_at > created_at),
  consumed_at  INTEGER CHECK(
    consumed_at IS NULL OR (consumed_at >= created_at AND consumed_at < expires_at)
  )
) STRICT;

CREATE INDEX idx_one_use_invite_links_unused_expiry
  ON one_use_invite_links(expires_at)
  WHERE consumed_at IS NULL;

CREATE TRIGGER one_use_invite_links_consumed_once
BEFORE UPDATE OF consumed_at ON one_use_invite_links
WHEN OLD.consumed_at IS NOT NULL AND NEW.consumed_at IS NOT OLD.consumed_at
BEGIN
  SELECT RAISE(ABORT, 'one-use invite already consumed');
END;
