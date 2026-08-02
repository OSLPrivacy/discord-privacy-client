-- T21-C1 reserved 0041..0043 for the ciphertext-only Space lane.
-- T21-C5: no account, roster, Space id, or plaintext event is stored here.
CREATE TABLE IF NOT EXISTS space_event_queue (
  id BLOB PRIMARY KEY NOT NULL,
  recipient_tag BLOB NOT NULL,
  ciphertext BLOB NOT NULL,
  expires_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_space_event_queue_tag
  ON space_event_queue(recipient_tag, created_at);
CREATE INDEX IF NOT EXISTS idx_space_event_queue_expiry
  ON space_event_queue(expires_at);
