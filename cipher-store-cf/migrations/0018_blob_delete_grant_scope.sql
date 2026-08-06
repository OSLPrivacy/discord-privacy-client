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
