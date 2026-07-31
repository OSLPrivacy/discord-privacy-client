-- 0007: atomic link-grant single-use consumption.
--
-- DEPLOY ORDER. Apply this migration BEFORE deploying the matching Worker.
-- Consumption is claimed by INSERT success, not by a prior read. A Worker
-- deployed before this table exists fails closed for link creation instead of
-- admitting a grant whose replay status cannot be recorded.

CREATE TABLE link_grant_consumed (
  jti TEXT PRIMARY KEY NOT NULL CHECK(length(jti) = 32 AND jti NOT GLOB '*[^0-9a-f]*'),
  expires_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_link_grant_consumed_expires_at
  ON link_grant_consumed(expires_at);
