/// Typed D1 query helpers. Mirrors the function surface of
/// keyserver/src/db.js, with D1's `prepare().bind().first/.all/.run`
/// idiom in place of better-sqlite3's `prepare(...).get/.run`.
///
/// Critical port: `popPrekeyBundle` is implemented via `db.batch()`
/// so the SELECT-then-DELETE remains transactional (D1's `prepare/run`
/// cycles are NOT transactional across calls; only `batch()` is).

export interface UserRow {
  user_id: string;
  ik_x25519_pub: string;
  ik_ed25519_pub: string;
  ik_mlkem768_pub: string;
  ik_x25519_signature: string;
  registered_at: string;
  last_rotated_at: string | null;
  ik_ratchet_initial_pub: string | null;
}

export interface RegisterInput {
  user_id: string;
  ik_x25519_pub: string;
  ik_ed25519_pub: string;
  ik_mlkem768_pub: string;
  ik_x25519_signature: string;
  ik_ratchet_initial_pub?: string | null;
}

export interface UpsertResult {
  isNew: boolean;
  registered_at?: string;
  last_rotated_at?: string;
}

export async function upsertUser(
  db: D1Database,
  input: RegisterInput,
): Promise<UpsertResult> {
  const now = new Date().toISOString();
  const ratchetPub = input.ik_ratchet_initial_pub ?? null;
  const existing = await db
    .prepare("SELECT user_id FROM users WHERE user_id = ?")
    .bind(input.user_id)
    .first<{ user_id: string }>();
  if (existing) {
    await db
      .prepare(
        `UPDATE users
            SET ik_x25519_pub = ?2,
                ik_ed25519_pub = ?3,
                ik_mlkem768_pub = ?4,
                ik_x25519_signature = ?5,
                ik_ratchet_initial_pub = ?6,
                last_rotated_at = ?7
          WHERE user_id = ?1`,
      )
      .bind(
        input.user_id,
        input.ik_x25519_pub,
        input.ik_ed25519_pub,
        input.ik_mlkem768_pub,
        input.ik_x25519_signature,
        ratchetPub,
        now,
      )
      .run();
    return { isNew: false, last_rotated_at: now };
  }
  await db
    .prepare(
      `INSERT INTO users
         (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
          ik_x25519_signature, ik_ratchet_initial_pub,
          registered_at, last_rotated_at)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)`,
    )
    .bind(
      input.user_id,
      input.ik_x25519_pub,
      input.ik_ed25519_pub,
      input.ik_mlkem768_pub,
      input.ik_x25519_signature,
      ratchetPub,
      now,
    )
    .run();
  return { isNew: true, registered_at: now };
}

export interface PubkeysRow {
  user_id: string;
  ik_x25519_pub: string;
  ik_ed25519_pub: string;
  ik_mlkem768_pub: string;
  ik_ratchet_initial_pub: string | null;
  registered_at: string;
  last_rotated_at: string | null;
}

export async function getUserPubkeys(
  db: D1Database,
  userId: string,
): Promise<PubkeysRow | null> {
  return await db
    .prepare(
      `SELECT user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
              ik_ratchet_initial_pub, registered_at, last_rotated_at
         FROM users WHERE user_id = ?`,
    )
    .bind(userId)
    .first<PubkeysRow>();
}

/** Variant that also returns ik_ed25519_pub for signature verification. */
export async function getUserForVerify(
  db: D1Database,
  userId: string,
): Promise<{ user_id: string; ik_ed25519_pub: string } | null> {
  return await db
    .prepare("SELECT user_id, ik_ed25519_pub FROM users WHERE user_id = ?")
    .bind(userId)
    .first<{ user_id: string; ik_ed25519_pub: string }>();
}

// ---- wrapped keys ----

export interface WrappedKeyRow {
  content_id: string;
  content_type: string;
  system_message_kind: string | null;
  sender_id: string;
  recipient_id: string;
  session_version: number;
  share_index: number;
  wrapped_share_blob: string;
  blob_version: number;
  single_use: number;
  display_duration_seconds: number | null;
  expires_at: string;
  created_at: string;
}

export interface InsertWrappedKeyInput {
  content_id: string;
  content_type: string;
  system_message_kind: string | null;
  sender_id: string;
  recipient_id: string;
  session_version: number;
  share_index: number;
  wrapped_share_blob: string;
  blob_version: number;
  single_use: number;
  display_duration_seconds: number | null;
  expires_at: string;
}

/** Raised when the content_id already exists. Wraps D1's constraint error. */
export class ContentIdConflict extends Error {}

export async function insertWrappedKey(
  db: D1Database,
  row: InsertWrappedKeyInput,
): Promise<void> {
  const now = new Date().toISOString();
  try {
    await db
      .prepare(
        `INSERT INTO wrapped_keys
           (content_id, content_type, system_message_kind,
            sender_id, recipient_id, session_version, share_index,
            wrapped_share_blob, blob_version, single_use,
            display_duration_seconds, expires_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)`,
      )
      .bind(
        row.content_id,
        row.content_type,
        row.system_message_kind,
        row.sender_id,
        row.recipient_id,
        row.session_version,
        row.share_index,
        row.wrapped_share_blob,
        row.blob_version,
        row.single_use,
        row.display_duration_seconds,
        row.expires_at,
        now,
      )
      .run();
  } catch (err) {
    // D1 surfaces SQLite errors as messages; the PK collision string
    // contains "UNIQUE constraint failed: wrapped_keys.content_id".
    const msg = err instanceof Error ? err.message : String(err);
    if (/UNIQUE\s+constraint\s+failed/i.test(msg)) {
      throw new ContentIdConflict("content_id already exists");
    }
    throw err;
  }
}

export type FetchWrappedKeyResult =
  | { status: "ok"; row: WrappedKeyRow & { single_use: boolean } }
  | { status: "gone" }
  | { status: "not_found" };

/**
 * Read a wrapped-key row, atomically tombstoning past-expiry rows
 * and atomically deleting single-use rows on first read. The
 * SELECT + DELETE pair runs inside `db.batch([...])` to keep the
 * pop atomic (D1's guarantee).
 */
export async function fetchWrappedKey(
  db: D1Database,
  contentId: string,
): Promise<FetchWrappedKeyResult> {
  // Step 1: read the row (cheap, before deciding what to delete).
  const row = await db
    .prepare(
      `SELECT content_id, content_type, system_message_kind,
              sender_id, recipient_id, session_version, share_index,
              wrapped_share_blob, blob_version, single_use,
              display_duration_seconds, expires_at, created_at
         FROM wrapped_keys WHERE content_id = ?`,
    )
    .bind(contentId)
    .first<WrappedKeyRow>();
  if (!row) return { status: "not_found" };
  const expired = Date.parse(row.expires_at) <= Date.now();
  if (expired) {
    await db
      .prepare("DELETE FROM wrapped_keys WHERE content_id = ?")
      .bind(contentId)
      .run();
    return { status: "gone" };
  }
  if (row.single_use) {
    // Atomic pop: delete by content_id ONLY when single_use is set
    // in the row we just read. If a concurrent fetcher already
    // claimed it, the DELETE is a no-op (changes=0); D1's row
    // already gone means we lost the race and the row is consumed
    // on this side too. We surface as `not_found` for the loser.
    const del = await db
      .prepare("DELETE FROM wrapped_keys WHERE content_id = ? AND single_use = 1")
      .bind(contentId)
      .run();
    const deleted = (del.meta?.changes ?? 0) > 0;
    if (!deleted) return { status: "not_found" };
  }
  return {
    status: "ok",
    row: { ...row, single_use: row.single_use === 1 } as WrappedKeyRow & {
      single_use: boolean;
    },
  };
}

// ---- burn ----

export type BurnScopeStr = "single" | "to_user" | "all";

export async function burnWrappedKeys(
  db: D1Database,
  burningUserId: string,
  scope: BurnScopeStr,
  target: { content_id?: string; user_id?: string } | null,
): Promise<{ deleted_count: number }> {
  let result: D1Result;
  if (scope === "single") {
    if (!target?.content_id) throw new Error("burn scope=single needs content_id");
    result = await db
      .prepare("DELETE FROM wrapped_keys WHERE content_id = ? AND sender_id = ?")
      .bind(target.content_id, burningUserId)
      .run();
  } else if (scope === "to_user") {
    if (!target?.user_id) throw new Error("burn scope=to_user needs user_id");
    result = await db
      .prepare("DELETE FROM wrapped_keys WHERE sender_id = ? AND recipient_id = ?")
      .bind(burningUserId, target.user_id)
      .run();
  } else {
    result = await db
      .prepare("DELETE FROM wrapped_keys WHERE sender_id = ?")
      .bind(burningUserId)
      .run();
  }
  return { deleted_count: result.meta?.changes ?? 0 };
}

// ---- prekey bundles ----

export interface SpkInput {
  pub_b64: string;
  signature_b64: string;
  rotated_at: string;
}

export interface OpkInput {
  id: number;
  pub_b64: string;
}

/**
 * Atomic SPK rotation + OPK append. Runs all statements inside
 * `db.batch()` so a constraint failure on any OPK rolls back the
 * SPK rotation too (matches the Railway transaction guarantee).
 */
export async function upsertPrekeyBundle(
  db: D1Database,
  userId: string,
  spk: SpkInput | null,
  opks: OpkInput[],
): Promise<void> {
  const statements: D1PreparedStatement[] = [];
  if (spk) {
    // We can't do a single UPSERT here because we need the PREVIOUS
    // row's spk_pub/sig/rotated_at to seed prev_*. The pattern used
    // by keyserver/src/db.js is SELECT-then-UPDATE/INSERT; we read
    // first (a separate call), then queue the write in the batch.
    const existing = await db
      .prepare(
        `SELECT spk_pub, spk_signature, spk_rotated_at
           FROM prekey_bundles WHERE user_id = ?`,
      )
      .bind(userId)
      .first<{ spk_pub: string; spk_signature: string; spk_rotated_at: string }>();
    if (existing) {
      statements.push(
        db
          .prepare(
            `UPDATE prekey_bundles
                SET prev_spk_pub = ?2,
                    prev_spk_signature = ?3,
                    prev_spk_rotated_at = ?4,
                    spk_pub = ?5,
                    spk_signature = ?6,
                    spk_rotated_at = ?7
              WHERE user_id = ?1`,
          )
          .bind(
            userId,
            existing.spk_pub,
            existing.spk_signature,
            existing.spk_rotated_at,
            spk.pub_b64,
            spk.signature_b64,
            spk.rotated_at,
          ),
      );
    } else {
      statements.push(
        db
          .prepare(
            `INSERT INTO prekey_bundles
               (user_id, spk_pub, spk_signature, spk_rotated_at)
             VALUES (?1, ?2, ?3, ?4)`,
          )
          .bind(userId, spk.pub_b64, spk.signature_b64, spk.rotated_at),
      );
    }
  }
  for (const o of opks) {
    statements.push(
      db
        .prepare(
          "INSERT INTO opk_pool (user_id, opk_id, opk_pub) VALUES (?, ?, ?)",
        )
        .bind(userId, o.id, o.pub_b64),
    );
  }
  if (statements.length === 0) return;
  try {
    await db.batch(statements);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    if (/UNIQUE\s+constraint\s+failed/i.test(msg)) {
      throw new OpkIdConflict("opk id already used");
    }
    throw err;
  }
}

export class OpkIdConflict extends Error {}

export interface PrekeyBundleResponse {
  user_id: string;
  ik_x25519_pub: string;
  ik_ed25519_pub: string;
  ik_mlkem768_pub: string;
  ik_ratchet_initial_pub: string | null;
  spk_pub: string;
  spk_signature: string;
  spk_rotated_at: string;
  opk: { id: number; pub_b64: string } | null;
  remaining_opk_count: number;
}

/**
 * Atomic pop. The OPK pick + delete is a single SQL statement
 *
 *   DELETE FROM opk_pool
 *    WHERE user_id = ?1
 *      AND opk_id = (SELECT MIN(opk_id) FROM opk_pool WHERE user_id = ?1)
 *   RETURNING opk_id, opk_pub
 *
 * which SQLite executes atomically (the subquery is evaluated
 * inside the DELETE's snapshot). Two concurrent calls cannot both
 * delete the same row; the loser's DELETE matches zero rows and
 * its RETURNING yields no record, so it falls through to the
 * "pool exhausted" branch (which then surfaces a different OPK or
 * the empty-pool fallback on its next call).
 *
 * The pop + count-remaining run as a `db.batch([...])` so the
 * `remaining_opk_count` we return is consistent with the pop
 * (both observe the same post-delete state under D1's batch
 * transaction guarantee).
 *
 * Returns `null` when the user has no identity row OR no SPK row.
 * `opk = null` means the pool was empty (design-doc OPK-exhaustion
 * fallback — senders skip DH4 and proceed with PQXDH).
 */
export async function popPrekeyBundle(
  db: D1Database,
  userId: string,
): Promise<PrekeyBundleResponse | null> {
  // Identity + SPK are read up front; they don't mutate during the
  // pop so they don't need to be inside the batch.
  const [userRow, spkRow] = await Promise.all([
    db
      .prepare(
        `SELECT user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
                ik_ratchet_initial_pub
           FROM users WHERE user_id = ?`,
      )
      .bind(userId)
      .first<{
        user_id: string;
        ik_x25519_pub: string;
        ik_ed25519_pub: string;
        ik_mlkem768_pub: string;
        ik_ratchet_initial_pub: string | null;
      }>(),
    db
      .prepare(
        `SELECT spk_pub, spk_signature, spk_rotated_at
           FROM prekey_bundles WHERE user_id = ?`,
      )
      .bind(userId)
      .first<{ spk_pub: string; spk_signature: string; spk_rotated_at: string }>(),
  ]);
  if (!userRow || !spkRow) return null;

  // Atomic pop + count-remaining inside a single D1 batch.
  const popStmt = db
    .prepare(
      `DELETE FROM opk_pool
        WHERE user_id = ?1
          AND opk_id = (SELECT MIN(opk_id) FROM opk_pool WHERE user_id = ?1)
        RETURNING opk_id, opk_pub`,
    )
    .bind(userId);
  const countStmt = db
    .prepare("SELECT COUNT(*) AS c FROM opk_pool WHERE user_id = ?")
    .bind(userId);
  const batchResults = await db.batch<{ opk_id?: number; opk_pub?: string; c?: number }>([
    popStmt,
    countStmt,
  ]);
  const popRes = batchResults[0];
  const countRes = batchResults[1];
  const popped = popRes?.results?.[0] as { opk_id: number; opk_pub: string } | undefined;
  const remaining = ((countRes?.results?.[0] as { c: number } | undefined)?.c) ?? 0;
  const consumedOpk = popped
    ? { id: popped.opk_id, pub_b64: popped.opk_pub }
    : null;

  return {
    user_id: userRow.user_id,
    ik_x25519_pub: userRow.ik_x25519_pub,
    ik_ed25519_pub: userRow.ik_ed25519_pub,
    ik_mlkem768_pub: userRow.ik_mlkem768_pub,
    ik_ratchet_initial_pub: userRow.ik_ratchet_initial_pub ?? null,
    spk_pub: spkRow.spk_pub,
    spk_signature: spkRow.spk_signature,
    spk_rotated_at: spkRow.spk_rotated_at,
    opk: consumedOpk,
    remaining_opk_count: remaining,
  };
}
