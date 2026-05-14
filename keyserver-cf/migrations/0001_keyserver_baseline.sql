-- F1.1 baseline schema. Ported 1:1 from keyserver/src/db.js so
-- existing Rust client code (crates/keystore/src/client.rs) works
-- against this worker with zero wire changes.
--
-- D1 supports WITHOUT ROWID, foreign keys, indexes, and the same
-- core SQLite syntax better-sqlite3 used. Differences vs the
-- Railway server:
--   - `pragma journal_mode = WAL` is managed by D1; no-op here.
--   - The pragma_table_info-based A2 migration was an additive
--     ALTER on the existing prod DB; in D1 we ship a fresh schema
--     that already has `ik_ratchet_initial_pub`. Pre-A2 records
--     don't exist in this DB because the cutover is a clean start
--     (per F1 discovery: no data migration; clients re-register).
--
-- Schema is forward-only; future migrations add tables/columns,
-- never modify or drop these.

CREATE TABLE users (
  user_id TEXT PRIMARY KEY,
  ik_x25519_pub TEXT NOT NULL,
  ik_ed25519_pub TEXT NOT NULL,
  ik_mlkem768_pub TEXT NOT NULL,
  ik_x25519_signature TEXT NOT NULL,
  registered_at TEXT NOT NULL,
  last_rotated_at TEXT,
  -- Phase 9-A2: published Double Ratchet bootstrap pub. NULL is
  -- the documented "peer not v=4 eligible" sentinel; senders fall
  -- through to v=3.
  ik_ratchet_initial_pub TEXT
) WITHOUT ROWID;

CREATE TABLE wrapped_keys (
  content_id TEXT PRIMARY KEY,
  content_type TEXT NOT NULL,
  system_message_kind TEXT,
  sender_id TEXT NOT NULL,
  recipient_id TEXT NOT NULL,
  session_version INTEGER NOT NULL,
  share_index INTEGER NOT NULL,
  wrapped_share_blob TEXT NOT NULL,
  blob_version INTEGER NOT NULL,
  single_use INTEGER NOT NULL,
  display_duration_seconds INTEGER,
  expires_at TEXT NOT NULL,
  created_at TEXT NOT NULL
) WITHOUT ROWID;

CREATE INDEX idx_wrapped_keys_recipient
  ON wrapped_keys (recipient_id);

-- B4: prekey infrastructure. Per-user signed prekey + (optional)
-- previous SPK retained for one rotation period; per-user OPK
-- pool. SPK is signed by IK_Ed25519 (verified on replenish). OPKs
-- are popped one at a time by GET /v1/prekey-bundle/:user_id; the
-- pop+remaining-count read is atomic via db.batch() in the worker.
CREATE TABLE prekey_bundles (
  user_id TEXT PRIMARY KEY,
  spk_pub TEXT NOT NULL,
  spk_signature TEXT NOT NULL,
  spk_rotated_at TEXT NOT NULL,
  prev_spk_pub TEXT,
  prev_spk_signature TEXT,
  prev_spk_rotated_at TEXT,
  FOREIGN KEY (user_id) REFERENCES users (user_id)
) WITHOUT ROWID;

CREATE TABLE opk_pool (
  user_id TEXT NOT NULL,
  opk_id INTEGER NOT NULL,
  opk_pub TEXT NOT NULL,
  PRIMARY KEY (user_id, opk_id),
  FOREIGN KEY (user_id) REFERENCES users (user_id)
);
