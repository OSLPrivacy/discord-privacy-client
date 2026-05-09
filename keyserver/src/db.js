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
      ik_ed25519_pub TEXT NOT NULL,
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

    -- B4: prekey infrastructure. Per-user signed prekey + (optional)
    -- previous SPK retained for one rotation period; per-user OPK
    -- pool. SPK is signed by the user's IK_Ed25519 (verified on
    -- replenish). OPKs are popped one at a time by GET
    -- /v1/prekey-bundle/:user_id.
    CREATE TABLE IF NOT EXISTS prekey_bundles (
      user_id TEXT PRIMARY KEY,
      spk_pub TEXT NOT NULL,
      spk_signature TEXT NOT NULL,
      spk_rotated_at TEXT NOT NULL,
      prev_spk_pub TEXT,
      prev_spk_signature TEXT,
      prev_spk_rotated_at TEXT,
      FOREIGN KEY (user_id) REFERENCES users (user_id)
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS opk_pool (
      user_id TEXT NOT NULL,
      opk_id INTEGER NOT NULL,
      opk_pub TEXT NOT NULL,
      PRIMARY KEY (user_id, opk_id),
      FOREIGN KEY (user_id) REFERENCES users (user_id)
    );
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
             ik_ed25519_pub = @ik_ed25519_pub,
             ik_mlkem768_pub = @ik_mlkem768_pub,
             ik_x25519_signature = @ik_x25519_signature,
             last_rotated_at = @now
       WHERE user_id = @user_id`,
    ).run({ ...row, now });
    return { isNew: false, last_rotated_at: now };
  }
  db.prepare(
    `INSERT INTO users
       (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
        ik_x25519_signature, registered_at, last_rotated_at)
     VALUES
       (@user_id, @ik_x25519_pub, @ik_ed25519_pub, @ik_mlkem768_pub,
        @ik_x25519_signature, @now, NULL)`,
  ).run({ ...row, now });
  return { isNew: true, registered_at: now };
}

export function getUser(db, userId) {
  return db
    .prepare(
      `SELECT user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
              registered_at, last_rotated_at
         FROM users WHERE user_id = ?`,
    )
    .get(userId);
}

// ---- prekey bundles ----

// Replace SPK + (optionally) append OPKs. `spk` may be null if the
// caller is only adding to the OPK pool. `opks` is an array of
// { id, pub_b64 }; ids must be unique per user (DB enforces).
export function upsertPrekeyBundle(db, userId, spk, opks) {
  const txn = db.transaction(() => {
    if (spk) {
      const existing = db
        .prepare('SELECT * FROM prekey_bundles WHERE user_id = ?')
        .get(userId);
      if (existing) {
        db.prepare(
          `UPDATE prekey_bundles
             SET prev_spk_pub = @existing_pub,
                 prev_spk_signature = @existing_sig,
                 prev_spk_rotated_at = @existing_rotated_at,
                 spk_pub = @spk_pub,
                 spk_signature = @spk_signature,
                 spk_rotated_at = @spk_rotated_at
             WHERE user_id = @user_id`,
        ).run({
          user_id: userId,
          existing_pub: existing.spk_pub,
          existing_sig: existing.spk_signature,
          existing_rotated_at: existing.spk_rotated_at,
          spk_pub: spk.pub_b64,
          spk_signature: spk.signature_b64,
          spk_rotated_at: spk.rotated_at,
        });
      } else {
        db.prepare(
          `INSERT INTO prekey_bundles
             (user_id, spk_pub, spk_signature, spk_rotated_at)
           VALUES (@user_id, @spk_pub, @spk_signature, @spk_rotated_at)`,
        ).run({
          user_id: userId,
          spk_pub: spk.pub_b64,
          spk_signature: spk.signature_b64,
          spk_rotated_at: spk.rotated_at,
        });
      }
    }
    for (const o of opks) {
      db.prepare(
        `INSERT INTO opk_pool (user_id, opk_id, opk_pub)
         VALUES (?, ?, ?)`,
      ).run(userId, o.id, o.pub_b64);
    }
  });
  txn();
}

// Atomically pop one OPK and return it along with the remaining
// pool size and the user's identity / SPK. Returns null if the user
// has no prekey bundle yet (callers treat as 404).
export function popPrekeyBundle(db, userId) {
  const txn = db.transaction(() => {
    const user = db
      .prepare(
        `SELECT user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub
           FROM users WHERE user_id = ?`,
      )
      .get(userId);
    if (!user) return null;
    const spk = db
      .prepare(
        `SELECT spk_pub, spk_signature, spk_rotated_at
           FROM prekey_bundles WHERE user_id = ?`,
      )
      .get(userId);
    if (!spk) return null;
    const opk = db
      .prepare(
        `SELECT opk_id, opk_pub FROM opk_pool
           WHERE user_id = ?
           ORDER BY opk_id ASC
           LIMIT 1`,
      )
      .get(userId);
    if (opk) {
      db.prepare(
        'DELETE FROM opk_pool WHERE user_id = ? AND opk_id = ?',
      ).run(userId, opk.opk_id);
    }
    const remaining = db
      .prepare('SELECT COUNT(*) AS c FROM opk_pool WHERE user_id = ?')
      .get(userId).c;
    return {
      user_id: user.user_id,
      ik_x25519_pub: user.ik_x25519_pub,
      ik_ed25519_pub: user.ik_ed25519_pub,
      ik_mlkem768_pub: user.ik_mlkem768_pub,
      spk_pub: spk.spk_pub,
      spk_signature: spk.spk_signature,
      spk_rotated_at: spk.spk_rotated_at,
      opk: opk ? { id: opk.opk_id, pub_b64: opk.opk_pub } : null,
      remaining_opk_count: remaining,
    };
  });
  return txn();
}

export function opkPoolSize(db, userId) {
  const r = db
    .prepare('SELECT COUNT(*) AS c FROM opk_pool WHERE user_id = ?')
    .get(userId);
  return r ? r.c : 0;
}

// ---- burn ----
//
// All burns delete only rows where `sender_id = burning_user_id` —
// you can't burn another user's content.
//
//   scope = 'single'   → match (sender, content_id)
//   scope = 'to_user'  → match (sender, recipient = target_user)
//   scope = 'all'      → match (sender)
//
// Returns { deleted_count }.
export function burnWrappedKeys(db, burningUserId, scope, target) {
  let stmt;
  let params;
  if (scope === 'single') {
    stmt = db.prepare(
      'DELETE FROM wrapped_keys WHERE content_id = ? AND sender_id = ?',
    );
    params = [target.content_id, burningUserId];
  } else if (scope === 'to_user') {
    stmt = db.prepare(
      'DELETE FROM wrapped_keys WHERE sender_id = ? AND recipient_id = ?',
    );
    params = [burningUserId, target.user_id];
  } else if (scope === 'all') {
    stmt = db.prepare('DELETE FROM wrapped_keys WHERE sender_id = ?');
    params = [burningUserId];
  } else {
    throw new Error(`unknown burn scope: ${scope}`);
  }
  const info = stmt.run(...params);
  return { deleted_count: info.changes };
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
