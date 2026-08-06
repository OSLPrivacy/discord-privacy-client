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
