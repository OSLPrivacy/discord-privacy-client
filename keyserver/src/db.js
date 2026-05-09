// Sqlite schema + helpers for the prototype key server.
//
// Schema mirrors the subset of `docs/design/key-server-api.md` we
// implement at this layer: identity-key registration + wrapped-key
// blob storage. Auth (Discord OAuth, signed re-registration), rate
// limiting, and prekey infrastructure are deferred per `build-order.md`.
//
// INSECURE BY DESIGN:
// - No auth on any endpoint.
// - Plain HTTP (no TLS).
// - No signature verification on registration.
// - Sqlite stored in plain file on disk.
// All called out in /v1 server module docs and /docs/design/.

import Database from 'better-sqlite3';

export function openDatabase(filename = ':memory:') {
  const db = new Database(filename);
  db.pragma('journal_mode = WAL');
  db.pragma('foreign_keys = ON');
  initSchema(db);
  return db;
}

function initSchema(db) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS users (
      user_id TEXT PRIMARY KEY,
      ik_x25519_pub TEXT NOT NULL,
      ik_mlkem768_pub TEXT NOT NULL,
      ik_x25519_signature TEXT NOT NULL,
      registered_at TEXT NOT NULL,
      last_rotated_at TEXT
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS wrapped_keys (
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

    CREATE INDEX IF NOT EXISTS idx_wrapped_keys_recipient
      ON wrapped_keys (recipient_id);
  `);
}

// ---- users ----

export function upsertUser(db, row) {
  const existing = db
    .prepare('SELECT user_id FROM users WHERE user_id = ?')
    .get(row.user_id);
  const now = new Date().toISOString();
  if (existing) {
    db.prepare(
      `UPDATE users
         SET ik_x25519_pub = @ik_x25519_pub,
             ik_mlkem768_pub = @ik_mlkem768_pub,
             ik_x25519_signature = @ik_x25519_signature,
             last_rotated_at = @now
       WHERE user_id = @user_id`,
    ).run({ ...row, now });
    return { isNew: false, last_rotated_at: now };
  }
  db.prepare(
    `INSERT INTO users
       (user_id, ik_x25519_pub, ik_mlkem768_pub, ik_x25519_signature,
        registered_at, last_rotated_at)
     VALUES
       (@user_id, @ik_x25519_pub, @ik_mlkem768_pub, @ik_x25519_signature,
        @now, NULL)`,
  ).run({ ...row, now });
  return { isNew: true, registered_at: now };
}

export function getUser(db, userId) {
  return db
    .prepare(
      `SELECT user_id, ik_x25519_pub, ik_mlkem768_pub,
              registered_at, last_rotated_at
         FROM users WHERE user_id = ?`,
    )
    .get(userId);
}

// ---- wrapped keys ----

export function insertWrappedKey(db, row) {
  const now = new Date().toISOString();
  db.prepare(
    `INSERT INTO wrapped_keys
       (content_id, content_type, system_message_kind,
        sender_id, recipient_id, session_version, share_index,
        wrapped_share_blob, blob_version, single_use,
        display_duration_seconds, expires_at, created_at)
     VALUES
       (@content_id, @content_type, @system_message_kind,
        @sender_id, @recipient_id, @session_version, @share_index,
        @wrapped_share_blob, @blob_version, @single_use,
        @display_duration_seconds, @expires_at, @now)`,
  ).run({ ...row, now });
}

// Returns one of:
//   { status: 'ok', row }
//   { status: 'gone' }       — tombstoned (past expires_at)
//   { status: 'not_found' }  — never existed or already burned/consumed
//
// For `single_use` rows the read is atomic with deletion: the
// transaction reads + deletes in one shot so concurrent fetchers
// only one wins.
export function fetchWrappedKey(db, contentId) {
  const txn = db.transaction((id) => {
    const row = db
      .prepare(
        `SELECT content_id, content_type, system_message_kind,
                sender_id, recipient_id, session_version, share_index,
                wrapped_share_blob, blob_version, single_use,
                display_duration_seconds, expires_at, created_at
           FROM wrapped_keys WHERE content_id = ?`,
      )
      .get(id);
    if (!row) {
      return { status: 'not_found' };
    }
    if (Date.parse(row.expires_at) <= Date.now()) {
      // Lazy-tombstone: drop the expired row and report gone.
      db.prepare('DELETE FROM wrapped_keys WHERE content_id = ?').run(id);
      return { status: 'gone' };
    }
    if (row.single_use) {
      db.prepare('DELETE FROM wrapped_keys WHERE content_id = ?').run(id);
    }
    // Normalise the boolean column for downstream consumers.
    row.single_use = Boolean(row.single_use);
    return { status: 'ok', row };
  });
  return txn(contentId);
}
