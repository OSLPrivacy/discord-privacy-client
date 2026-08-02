-- D1 Time Travel retains deleted cells. Rebuild the retired legacy table
-- without its ciphertext BLOB so no D1 column can hold message payload bytes.
ALTER TABLE blobs RENAME TO blobs_with_payload_retired;

CREATE TABLE blobs (
  id          BLOB    PRIMARY KEY NOT NULL,
  size_bytes  INTEGER NOT NULL,
  expires_at  INTEGER NOT NULL,
  created_at  INTEGER NOT NULL
) STRICT;

INSERT INTO blobs (id, size_bytes, expires_at, created_at)
  SELECT id, size_bytes, expires_at, created_at
  FROM blobs_with_payload_retired;

DROP TABLE blobs_with_payload_retired;
CREATE INDEX IF NOT EXISTS idx_blobs_expires_at ON blobs(expires_at);
