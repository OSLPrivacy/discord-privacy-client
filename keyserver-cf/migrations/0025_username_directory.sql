-- Public exact-match username directory. Usernames are deliberately not a
-- fuzzy-search surface: callers may resolve one already-normalized name only.
CREATE TABLE username_directory (
  username TEXT PRIMARY KEY,
  user_id TEXT NOT NULL UNIQUE,
  friend_code TEXT NOT NULL,
  claimed_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (user_id) REFERENCES users (user_id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE TABLE username_claim_receipts (
  user_id TEXT NOT NULL,
  request_digest BLOB NOT NULL,
  expires_at INTEGER NOT NULL,
  PRIMARY KEY (user_id, request_digest)
) WITHOUT ROWID;

CREATE INDEX idx_username_claim_receipts_expiry
  ON username_claim_receipts (expires_at);
