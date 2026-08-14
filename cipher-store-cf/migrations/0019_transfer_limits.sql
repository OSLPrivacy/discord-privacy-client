-- Per-client transfer controls. Client addresses are HMACed before reaching
-- these tables; neither table stores a raw address or a bearer capability.

CREATE TABLE transfer_upload_slots (
  client_key TEXT NOT NULL CHECK(length(client_key) = 64 AND client_key NOT GLOB '*[^0-9a-f]*'),
  slot_id TEXT NOT NULL CHECK(length(slot_id) = 32 AND slot_id NOT GLOB '*[^0-9a-f]*'),
  expires_at INTEGER NOT NULL,
  PRIMARY KEY (client_key, slot_id)
) STRICT;

CREATE INDEX idx_transfer_upload_slots_expires_at
  ON transfer_upload_slots(expires_at);

CREATE TABLE transfer_bandwidth_windows (
  client_key TEXT NOT NULL CHECK(length(client_key) = 64 AND client_key NOT GLOB '*[^0-9a-f]*'),
  direction TEXT NOT NULL CHECK(direction IN ('upload', 'download')),
  window_start_ms INTEGER NOT NULL,
  used_bytes INTEGER NOT NULL CHECK(used_bytes > 0),
  PRIMARY KEY (client_key, direction, window_start_ms)
) STRICT;

CREATE INDEX idx_transfer_bandwidth_windows_start
  ON transfer_bandwidth_windows(window_start_ms);
