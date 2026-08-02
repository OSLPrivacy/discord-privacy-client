-- Pointer-store r2 metadata.  Keep this index separate from the legacy
-- `blobs` table: old Worker versions continue to serve its short-lived rows
-- while the r2 Worker (introduced after this migration) uses this table.
--
-- This is deliberately metadata-only. Ciphertext moves to PAYLOADS in W4;
-- D1 must retain only capability digests and lifecycle/indexing fields.

CREATE TABLE blob_capability_index (
  blob_id                          TEXT    PRIMARY KEY NOT NULL
                                           CHECK(length(blob_id) = 32
                                             AND blob_id NOT GLOB '*[^0-9a-f]*'),
  fetch_digest_sha256_hex          TEXT    NOT NULL
                                           CHECK(length(fetch_digest_sha256_hex) = 64
                                             AND fetch_digest_sha256_hex NOT GLOB '*[^0-9a-f]*'),
  ack_digest_sha256_hex            TEXT    NOT NULL
                                           CHECK(length(ack_digest_sha256_hex) = 64
                                             AND ack_digest_sha256_hex NOT GLOB '*[^0-9a-f]*'),
  manage_digest_sha256_hex         TEXT    NOT NULL
                                           CHECK(length(manage_digest_sha256_hex) = 64
                                             AND manage_digest_sha256_hex NOT GLOB '*[^0-9a-f]*'),
  object_class                     TEXT    NOT NULL
                                           CHECK(object_class IN ('single-ack', 'multi-fetch')),
  pool                             TEXT    NOT NULL
                                           CHECK(pool = 'undelivered'),
  delivery_tag                     TEXT    NOT NULL
                                           CHECK(length(delivery_tag) = 32
                                             AND delivery_tag NOT GLOB '*[^0-9a-f]*'),
  size_bytes                       INTEGER NOT NULL CHECK(size_bytes > 0),
  expires_at                       INTEGER NOT NULL,
  created_at                       INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_blob_capability_index_expires_at
  ON blob_capability_index(expires_at);

CREATE INDEX idx_blob_capability_index_delivery_tag
  ON blob_capability_index(delivery_tag);
