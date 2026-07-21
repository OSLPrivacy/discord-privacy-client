-- Attachment storage has not shipped yet, so intentionally replace the empty
-- metadata table instead of copying raw bearer capabilities into a new table.
-- Only SHA-256 capability digests are retained in D1.

DROP TABLE IF EXISTS attachment_parts;
DROP TABLE IF EXISTS attachment_objects;

CREATE TABLE attachment_objects (
  id                     TEXT    PRIMARY KEY NOT NULL
                                 CHECK(length(id) = 32 AND id NOT GLOB '*[^0-9a-f]*'),
  object_key             TEXT    NOT NULL UNIQUE,
  size_bytes             INTEGER NOT NULL CHECK(size_bytes > 0 AND size_bytes <= 537919488),
  expires_at             INTEGER NOT NULL,
  created_at             INTEGER NOT NULL,
  fetch_token_sha256_hex TEXT    NOT NULL
                                 CHECK(length(fetch_token_sha256_hex) = 64
                                   AND fetch_token_sha256_hex NOT GLOB '*[^0-9a-f]*'),
  state                  TEXT    NOT NULL CHECK(state IN ('uploading', 'completing', 'ready')),
  upload_id              TEXT,
  CHECK((state = 'ready' AND upload_id IS NULL)
     OR (state IN ('uploading', 'completing') AND upload_id IS NOT NULL))
) STRICT;

CREATE INDEX idx_attachment_objects_expires_at
  ON attachment_objects(expires_at);

-- Aggregate row/byte quota and declared multipart-size enforcement are
-- performed by atomic conditional INSERT/UPSERT statements in the Worker.
-- Keeping the migration to ordinary DDL avoids Wrangler's statement splitter
-- treating trigger-body semicolons as incomplete standalone statements.
CREATE TABLE attachment_parts (
  attachment_id TEXT    NOT NULL REFERENCES attachment_objects(id) ON DELETE CASCADE,
  part_number   INTEGER NOT NULL CHECK(part_number BETWEEN 1 AND 65),
  size_bytes    INTEGER NOT NULL CHECK(size_bytes > 0 AND size_bytes <= 8388608),
  etag          TEXT    CHECK(etag IS NULL OR length(etag) BETWEEN 1 AND 256),
  PRIMARY KEY (attachment_id, part_number)
) STRICT;
