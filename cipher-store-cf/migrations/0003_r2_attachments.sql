-- Large encrypted attachments live in R2. D1 retains only the minimum
-- capability/expiry index needed to authorize reads and reclaim objects.
-- No identity, filename, MIME type, conversation, or plaintext metadata.

CREATE TABLE IF NOT EXISTS attachment_objects (
  id          TEXT    PRIMARY KEY NOT NULL CHECK(length(id) = 32),
  object_key  TEXT    NOT NULL UNIQUE,
  size_bytes  INTEGER NOT NULL CHECK(size_bytes > 0 AND size_bytes <= 27262976),
  expires_at  INTEGER NOT NULL,
  fetch_token TEXT    NOT NULL CHECK(length(fetch_token) = 32)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_attachment_objects_expires_at
  ON attachment_objects(expires_at);
