-- 0018: sender-scoped delete-grant metadata for protected message blobs.
--
-- These columns are nullable so older unscoped blob rows keep their existing
-- manage-capability deletion behavior. Rows that set any of the three fields
-- must set all of them; the Worker then requires an x-osl-delete-grant whose
-- typed record names the same message, owner, and burn scope before it will
-- evaluate the manage capability.

ALTER TABLE blob_capability_index
  ADD COLUMN delete_message TEXT;

ALTER TABLE blob_capability_index
  ADD COLUMN delete_owner TEXT;

ALTER TABLE blob_capability_index
  ADD COLUMN burn_scope TEXT;

CREATE INDEX idx_blob_capability_index_delete_scope
  ON blob_capability_index(delete_owner, burn_scope)
  WHERE delete_owner IS NOT NULL AND burn_scope IS NOT NULL;
-- Server-side delete-grant validation metadata.
--
-- These fields let DELETE /v1/blob/:id validate a presented delete grant
-- against server-held message, owner, and burn-scope context before any stored
-- copy can be destroyed. Existing rows stay NULL and fail closed until the
-- legacy-message migration marks them explicitly.

ALTER TABLE blob_capability_index
  ADD COLUMN delete_grant_message TEXT
  CHECK(delete_grant_message IS NULL OR (
    length(delete_grant_message) BETWEEN 1 AND 256
    AND delete_grant_message NOT GLOB '*[^A-Za-z0-9._:@/-]*'
  ));

ALTER TABLE blob_capability_index
  ADD COLUMN delete_grant_owner TEXT
  CHECK(delete_grant_owner IS NULL OR (
    length(delete_grant_owner) BETWEEN 1 AND 256
    AND delete_grant_owner NOT GLOB '*[^A-Za-z0-9._:@/-]*'
  ));

ALTER TABLE blob_capability_index
  ADD COLUMN burn_scope TEXT
  CHECK(burn_scope IS NULL OR (
    length(burn_scope) BETWEEN 1 AND 256
    AND burn_scope NOT GLOB '*[^A-Za-z0-9._:@/-]*'
  ));
