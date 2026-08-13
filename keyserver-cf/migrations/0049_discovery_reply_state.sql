-- 0049: authoritative discovery reply master switch.
--
-- A retained row is intentional: after an account turns replies off, an old
-- or racing publisher must not recreate a card merely because no card exists.

CREATE TABLE discovery_reply_state (
  account_id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;

INSERT INTO worker_schema_capabilities (capability, version)
VALUES ('discovery_reply_master_state', 1);
