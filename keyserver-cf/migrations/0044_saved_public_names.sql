-- 0044: minimal saved public-name records.
--
-- Public-name persistence is intentionally narrower than the operational
-- username directory.  A saved-name record may retain only the public name,
-- the public identity key that claimed it, the proof record, and the claim
-- time.

CREATE TABLE saved_names (
  public_name TEXT NOT NULL UNIQUE,
  public_identity_key TEXT PRIMARY KEY,
  proof_record TEXT NOT NULL,
  claimed_at TEXT NOT NULL
) WITHOUT ROWID;

INSERT OR REPLACE INTO worker_schema_capabilities (capability, version)
VALUES ('minimal_saved_public_names', 1);
