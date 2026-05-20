-- Phase 1 init: single table holding ciphertext blobs keyed by an
-- opaque random 8-byte ID. No identity columns -- the server
-- intentionally has no concept of "who uploaded this".

CREATE TABLE IF NOT EXISTS blobs (
  id          BLOB    PRIMARY KEY NOT NULL,  -- 8 random bytes
  data        BLOB    NOT NULL,              -- E2E ciphertext, server never reads
  size_bytes  INTEGER NOT NULL,              -- = length(data); for sweep stats
  expires_at  INTEGER NOT NULL,              -- unix-epoch seconds; sweep deletes when past
  created_at  INTEGER NOT NULL               -- unix-epoch seconds; debug only
) STRICT;

CREATE INDEX IF NOT EXISTS idx_blobs_expires_at ON blobs(expires_at);
