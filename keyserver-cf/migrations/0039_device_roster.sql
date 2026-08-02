-- Per-account delivery targets.  A device has no independent identity key:
-- it carries only an opaque registration id and its own prekey bundle.
CREATE TABLE device_roster (
  user_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  prekey_bundle TEXT NOT NULL,
  registered_at TEXT NOT NULL,
  PRIMARY KEY (user_id, device_id),
  FOREIGN KEY (user_id) REFERENCES users (user_id)
) WITHOUT ROWID;

CREATE INDEX idx_device_roster_user ON device_roster(user_id);
