-- Server-held authorization metadata for destructive stored-copy burns.
-- The bearer delete grant is only accepted when it matches the copy's locally
-- owned message, owner, and burn scope recorded at upload.

ALTER TABLE blob_capability_index
  ADD COLUMN delete_message TEXT;

ALTER TABLE blob_capability_index
  ADD COLUMN delete_owner TEXT;

ALTER TABLE blob_capability_index
  ADD COLUMN burn_scope TEXT;

CREATE INDEX idx_blob_capability_index_delete_scope
  ON blob_capability_index(delete_owner, burn_scope);
