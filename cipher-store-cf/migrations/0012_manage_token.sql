-- Separate the bearer capability used to retrieve ciphertext from the one
-- held by its sender to delete it. Existing rows intentionally remain NULL:
-- the endpoint treats them as undeletable rather than widening authority.
ALTER TABLE blobs ADD COLUMN manage_token TEXT;

