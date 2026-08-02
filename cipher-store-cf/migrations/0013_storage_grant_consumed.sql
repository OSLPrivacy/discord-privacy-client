-- 0013: atomic storage-grant single-use consumption.
--
-- Deploy this migration before a Worker that consumes storage grants. INSERT
-- success is the single-use claim; no read-before-write race is permitted.

CREATE TABLE storage_grant_consumed (
  jti TEXT PRIMARY KEY NOT NULL CHECK(length(jti) = 32 AND jti NOT GLOB '*[^0-9a-f]*'),
  expires_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_storage_grant_consumed_expires_at
  ON storage_grant_consumed(expires_at);
