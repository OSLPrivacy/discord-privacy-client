-- Monotonic version receipt for the root-signed device roster.
-- `device_roster` stores the current expanded rows; this table stores the
-- per-account list version that made those rows authoritative.
CREATE TABLE device_roster_versions (
  user_id TEXT PRIMARY KEY,
  version INTEGER NOT NULL CHECK (version >= 1),
  updated_at TEXT NOT NULL,
  FOREIGN KEY (user_id) REFERENCES users (user_id)
) WITHOUT ROWID;
