-- Records the updater's terminal install attempt outcome without storing a
-- user, device, IP address, or request path version data.
CREATE TABLE IF NOT EXISTS update_attempt_records (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  from_version TEXT NOT NULL CHECK (
    length(from_version) BETWEEN 1 AND 64
    AND from_version NOT GLOB '*[^A-Za-z0-9.+-]*'
  ),
  to_version TEXT NOT NULL CHECK (
    length(to_version) BETWEEN 1 AND 64
    AND to_version NOT GLOB '*[^A-Za-z0-9.+-]*'
  ),
  result TEXT NOT NULL CHECK (result IN ('installed', 'failed')),
  recorded_at_unix_ms INTEGER NOT NULL CHECK (recorded_at_unix_ms > 0)
);

CREATE INDEX IF NOT EXISTS idx_update_attempt_records_time
  ON update_attempt_records(recorded_at_unix_ms);
