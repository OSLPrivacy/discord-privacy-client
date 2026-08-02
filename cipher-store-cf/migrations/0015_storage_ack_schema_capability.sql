-- Release gates require this marker before a Worker may advertise the
-- receipt-aware pointer-store surface. The marker is additive so production
-- databases that already applied 0011 gain it through the normal migration
-- path rather than relying on a rewritten historical migration.
CREATE TABLE IF NOT EXISTS worker_schema_capabilities (
  capability TEXT PRIMARY KEY,
  version    INTEGER NOT NULL CHECK (version >= 1)
) WITHOUT ROWID;

INSERT INTO worker_schema_capabilities (capability, version)
VALUES ('storage_ack_v1', 1)
ON CONFLICT(capability) DO UPDATE SET version = excluded.version;
