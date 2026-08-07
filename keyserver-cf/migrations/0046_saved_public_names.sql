-- 0046: minimal public-name record, bound to the public identity key that
-- claimed it.
--
-- This table is intentionally narrow. It stores the searchable public name,
-- the Ed25519 public identity key that owns the name, the proof receipt used
-- for the current binding, and the claim time.

CREATE TABLE IF NOT EXISTS saved_names (
  public_name TEXT PRIMARY KEY,
  public_identity_key TEXT NOT NULL,
  proof_record TEXT NOT NULL,
  claimed_at TEXT NOT NULL
) WITHOUT ROWID;

INSERT OR REPLACE INTO worker_schema_capabilities (capability, version)
VALUES ('saved_public_names', 1);
