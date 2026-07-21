/// Typed D1 query helpers. Mirrors the function surface of
/// keyserver/src/db.js, with D1's `prepare().bind().first/.all/.run`
/// idiom in place of better-sqlite3's `prepare(...).get/.run`.
///
/// Critical port: authenticated prekey pops are implemented via `db.batch()`
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

/**
 * REGISTER-FIX (open signed register): explicit INSERT for a brand
 * new user_id (state-machine Case A). Separated from `upsertUser`'s
 * blind overwrite so register's first-write-wins / authenticated-
 * rotation logic is the *only* path that can mutate an existing row.
 * `registered_at = now`, `last_rotated_at = NULL`.
 */
export async function insertUser(
  db: D1Database,
  input: RegisterInput,
): Promise<{ registered_at: string }> {
  const now = new Date().toISOString();
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
      input.ik_ratchet_initial_pub ?? null,
      now,
    )
    .run();
  return { registered_at: now };
}

/**
 * REGISTER-FIX: authenticated key rotation (state-machine Case C).
 * Overwrites the key columns + bumps `last_rotated_at`; `registered_at`
 * is intentionally left untouched (it records first-ever registration).
 * The caller MUST have already verified the rotation is authorised by
 * the *currently-stored* ik_ed25519_pub before invoking this.
 */
export async function rotateUserKeys(
  db: D1Database,
  input: RegisterInput,
  expectedCurrentEd25519Pub: string,
): Promise<{ last_rotated_at: string } | null> {
  const now = new Date().toISOString();
  const results = await db.batch([
    db.prepare(
      `DELETE FROM username_directory
        WHERE user_id = ?1
          AND EXISTS (
            SELECT 1 FROM users
             WHERE user_id = ?1 AND ik_ed25519_pub = ?2
          )`,
    ).bind(input.user_id, expectedCurrentEd25519Pub),
    db.prepare(
      `UPDATE users
          SET ik_x25519_pub = ?2,
              ik_ed25519_pub = ?3,
              ik_mlkem768_pub = ?4,
              ik_x25519_signature = ?5,
              ik_ratchet_initial_pub = ?6,
              last_rotated_at = ?7
        WHERE user_id = ?1
          AND ik_ed25519_pub = ?8`,
    ).bind(
      input.user_id,
      input.ik_x25519_pub,
      input.ik_ed25519_pub,
      input.ik_mlkem768_pub,
      input.ik_x25519_signature,
      input.ik_ratchet_initial_pub ?? null,
      now,
      expectedCurrentEd25519Pub,
    ),
  ]);
  if ((results[1]?.meta?.changes ?? 0) !== 1) return null;
  return { last_rotated_at: now };
}

/**
 * Delete a user and all user-owned rows only while the identity key
 * authenticated by the caller is still current.
 *
 * D1 executes `batch` transactionally and in statement order. Every
 * live-data child delete repeats the key predicate and the parent CAS is last,
 * so a rotation between signature verification and this batch leaves
 * both the replacement identity and its data untouched. Child rows
 * are removed before `users` to satisfy the prekey foreign keys.
 * Short-lived replay receipts deliberately survive account deletion;
 * otherwise restoring the same key would make captured requests valid again.
 */
export type UnregisterUserResult = "deleted" | "replay" | "stale_identity";

export async function unregisterUserIfCurrent(
  db: D1Database,
  userId: string,
  expectedCurrentEd25519Pub: string,
  requestDigest: Uint8Array,
  receiptExpiresAt: number,
): Promise<UnregisterUserResult> {
  const ownsCurrentKey =
    "EXISTS (SELECT 1 FROM users WHERE user_id = ? AND ik_ed25519_pub = ?)";
  let results: D1Result[];
  try {
    results = await db.batch([
      db
        .prepare("DELETE FROM unregister_receipts WHERE expires_at < ?")
        .bind(Math.floor(Date.now() / 1000)),
      db
        .prepare(
          `INSERT INTO unregister_receipts
           (user_id, signer_ed25519_pub, request_digest, expires_at)
         SELECT ?1, ?2, ?3, ?4
          WHERE EXISTS (
            SELECT 1 FROM users
             WHERE user_id = ?1 AND ik_ed25519_pub = ?2
          )`,
        )
        .bind(userId, expectedCurrentEd25519Pub, requestDigest, receiptExpiresAt),
      db
        .prepare(
          `DELETE FROM control_inbox
          WHERE (recipient_id = ? OR sender_id = ?)
            AND ${ownsCurrentKey}`,
        )
        .bind(userId, userId, userId, expectedCurrentEd25519Pub),
      db
        .prepare(
          `DELETE FROM wrapped_keys
          WHERE (recipient_id = ? OR sender_id = ?)
            AND ${ownsCurrentKey}`,
        )
        .bind(userId, userId, userId, expectedCurrentEd25519Pub),
      db
        .prepare(
          `DELETE FROM username_directory
          WHERE user_id = ?
            AND ${ownsCurrentKey}`,
        )
        .bind(userId, userId, expectedCurrentEd25519Pub),
      db
        .prepare(
          `DELETE FROM opk_pool
          WHERE user_id = ?
            AND ${ownsCurrentKey}`,
        )
        .bind(userId, userId, expectedCurrentEd25519Pub),
      db
        .prepare(
          `DELETE FROM prekey_bundles
          WHERE user_id = ?
            AND ${ownsCurrentKey}`,
        )
        .bind(userId, userId, expectedCurrentEd25519Pub),
      db
        .prepare("DELETE FROM users WHERE user_id = ? AND ik_ed25519_pub = ?")
        .bind(userId, expectedCurrentEd25519Pub),
    ]);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (/unregister_receipts/i.test(message) && /UNIQUE|PRIMARY/i.test(message)) {
      return "replay";
    }
    throw err;
  }
  if ((results[1]?.meta?.changes ?? 0) !== 1) return "stale_identity";
  const parentDelete = results[results.length - 1];
  return (parentDelete?.meta?.changes ?? 0) === 1
    ? "deleted"
    : "stale_identity";
}

export interface PrivacyRetentionSweepResult {
  wrappedKeys: number;
  consumingGetReceipts: number;
  wrappedKeyPostReceipts: number;
  prekeyReplenishReceipts: number;
  wrappedKeyBurnReceipts: number;
  unregisterReceipts: number;
}

/**
 * Physically delete expired encrypted-key material and replay receipts.
 *
 * `wrapped_keys.expires_at` is ISO-8601 text supplied by signed clients, so
 * compare it through SQLite's timestamp parser instead of lexicographically.
 * The receipt tables use Unix seconds. D1 batches the deletes atomically.
 */
export async function sweepExpiredPrivacyRows(
  db: D1Database,
  nowMs = Date.now(),
): Promise<PrivacyRetentionSweepResult> {
  const nowSeconds = Math.floor(nowMs / 1000);
  const results = await db.batch([
    db
      .prepare(
        "DELETE FROM wrapped_keys WHERE unixepoch(expires_at) <= ? RETURNING content_id",
      )
      .bind(nowSeconds),
    db
      .prepare("DELETE FROM consuming_get_receipts WHERE expires_at <= ?")
      .bind(nowSeconds),
    db
      .prepare("DELETE FROM wrapped_key_post_receipts WHERE expires_at <= ?")
      .bind(nowSeconds),
    db
      .prepare("DELETE FROM prekey_replenish_receipts WHERE expires_at <= ?")
      .bind(nowSeconds),
    db
      .prepare("DELETE FROM wrapped_key_burn_receipts WHERE expires_at <= ?")
      .bind(nowSeconds),
    db
      .prepare("DELETE FROM unregister_receipts WHERE expires_at <= ?")
      .bind(nowSeconds),
  ]);
  return {
    // The storage-accounting DELETE trigger also changes its counter row, so
    // `meta.changes` is not the logical wrapped-key row count. RETURNING is.
    wrappedKeys: results[0]?.results?.length ?? 0,
    consumingGetReceipts: results[1]?.meta?.changes ?? 0,
    wrappedKeyPostReceipts: results[2]?.meta?.changes ?? 0,
    prekeyReplenishReceipts: results[3]?.meta?.changes ?? 0,
    wrappedKeyBurnReceipts: results[4]?.meta?.changes ?? 0,
    unregisterReceipts: results[5]?.meta?.changes ?? 0,
  };
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
export class WrappedKeyPostReplay extends Error {}
export class StaleSenderIdentity extends Error {}
export class WrappedKeySenderQuotaExceeded extends Error {}
export class UnknownWrappedKeyRecipient extends Error {}

/**
 * Insert an identity-authorized wrapped key and its replay receipt in one D1
 * transaction. The repeated identity-key predicate is a CAS: rotation between
 * endpoint verification and this write cannot spend authority from the old
 * key. A receipt outlives deletion of the wrapped row for twice the signature
 * freshness window, so a captured request cannot resurrect burned content.
 */
export async function insertWrappedKeyAuthenticated(
  db: D1Database,
  row: InsertWrappedKeyInput,
  requestDigest: Uint8Array,
  expectedSenderEd25519Pub: string,
  receiptExpiresAt: number,
): Promise<void> {
  const now = new Date().toISOString();
  const nowSeconds = Math.floor(Date.now() / 1000);
  const cleanup = db
    .prepare("DELETE FROM wrapped_key_post_receipts WHERE expires_at < ?")
    .bind(nowSeconds);
  const receipt = db
    .prepare(
      `INSERT INTO wrapped_key_post_receipts
         (sender_id, request_digest, content_id, expires_at)
       SELECT ?1, ?2, ?3, ?5
        WHERE EXISTS (
          SELECT 1 FROM users
           WHERE user_id = ?1 AND ik_ed25519_pub = ?4
        )
          AND EXISTS (
          SELECT 1 FROM users
           WHERE user_id = ?6
        )`,
    )
    .bind(
      row.sender_id,
      requestDigest,
      row.content_id,
      expectedSenderEd25519Pub,
      receiptExpiresAt,
      row.recipient_id,
    );
  const insert = db
    .prepare(
      `INSERT INTO wrapped_keys
         (content_id, content_type, system_message_kind,
          sender_id, recipient_id, session_version, share_index,
          wrapped_share_blob, blob_version, single_use,
          display_duration_seconds, expires_at, created_at)
       SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
        WHERE EXISTS (
          SELECT 1 FROM wrapped_key_post_receipts
           WHERE sender_id = ?4 AND request_digest = ?14
        )
          AND EXISTS (
          SELECT 1 FROM users
           WHERE user_id = ?4 AND ik_ed25519_pub = ?15
        )
          AND EXISTS (
          SELECT 1 FROM users
           WHERE user_id = ?5
        )`,
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
      requestDigest,
      expectedSenderEd25519Pub,
    );
  let results: D1Result[];
  try {
    results = await db.batch([cleanup, receipt, insert]);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (/wrapped_key_post_receipts/i.test(message) && /UNIQUE|PRIMARY/i.test(message)) {
      throw new WrappedKeyPostReplay("signed wrapped-key upload already used");
    }
    if (/wrapped_keys\.content_id/i.test(message) && /UNIQUE|PRIMARY/i.test(message)) {
      throw new ContentIdConflict("content_id already exists");
    }
    if (
      message.includes("wrapped key sender quota exceeded") ||
      message.includes("wrapped key recipient quota exceeded") ||
      message.includes("wrapped key global storage quota exceeded")
    ) {
      throw new WrappedKeySenderQuotaExceeded("wrapped key storage quota exceeded");
    }
    throw err;
  }
  if ((results[1]?.meta?.changes ?? 0) !== 1 || (results[2]?.meta?.changes ?? 0) < 1) {
    if (
      (await identityKeyIsCurrent(db, row.sender_id, expectedSenderEd25519Pub)) &&
      !(await userExists(db, row.recipient_id))
    ) {
      throw new UnknownWrappedKeyRecipient("wrapped-key recipient is not registered");
    }
    throw new StaleSenderIdentity("sender identity changed during authorization");
  }
}

async function userExists(db: D1Database, userId: string): Promise<boolean> {
  const row = await db
    .prepare("SELECT 1 AS ok FROM users WHERE user_id = ?")
    .bind(userId)
    .first<{ ok: number }>();
  return row?.ok === 1;
}

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

export type PublicWrappedKeyRow = Omit<WrappedKeyRow, "single_use"> & {
  single_use: boolean;
};

export type FetchWrappedKeyResult =
  | { status: "ok"; row: PublicWrappedKeyRow }
  | { status: "gone" }
  | { status: "not_found" }
  | { status: "stale_identity" };

export async function getWrappedKeyAccess(
  db: D1Database,
  contentId: string,
): Promise<{ recipient_id: string; single_use: number } | null> {
  return await db
    .prepare(
      "SELECT recipient_id, single_use FROM wrapped_keys WHERE content_id = ?",
    )
    .bind(contentId)
    .first<{ recipient_id: string; single_use: number }>();
}

/** Preserve the legacy public read contract for reusable rows only. */
export async function fetchReusableWrappedKey(
  db: D1Database,
  contentId: string,
): Promise<FetchWrappedKeyResult> {
  const row = await db
    .prepare(
      `SELECT content_id, content_type, system_message_kind,
              sender_id, recipient_id, session_version, share_index,
              wrapped_share_blob, blob_version, single_use,
              display_duration_seconds, expires_at, created_at
         FROM wrapped_keys
        WHERE content_id = ? AND single_use = 0`,
    )
    .bind(contentId)
    .first<WrappedKeyRow>();
  if (!row) return { status: "not_found" };
  if (Date.parse(row.expires_at) <= Date.now()) {
    await db
      .prepare(
        "DELETE FROM wrapped_keys WHERE content_id = ? AND single_use = 0",
      )
      .bind(contentId)
      .run();
    return { status: "gone" };
  }
  return { status: "ok", row: { ...row, single_use: false } };
}

/** Authenticated single-use pop with identity CAS and replay receipt. */
export async function fetchWrappedKeyAuthenticated(
  db: D1Database,
  contentId: string,
  recipientId: string,
  requestDigest: Uint8Array,
  expectedRecipientEd25519Pub: string,
): Promise<FetchWrappedKeyResult> {
  // Read only after the endpoint has authenticated the signed request.
  // The destructive branches repeat the recipient identity-key predicate
  // as a database CAS so a key rotation between verify and consume cannot
  // spend authority belonging to the replaced key.
  const row = await db
    .prepare(
      `SELECT content_id, content_type, system_message_kind,
              sender_id, recipient_id, session_version, share_index,
              wrapped_share_blob, blob_version, single_use,
              display_duration_seconds, expires_at, created_at
         FROM wrapped_keys
        WHERE content_id = ? AND recipient_id = ?`,
    )
    .bind(contentId, recipientId)
    .first<WrappedKeyRow>();
  if (!row) return { status: "not_found" };
  const expired = Date.parse(row.expires_at) <= Date.now();
  if (row.single_use) {
    const nowSeconds = Math.floor(Date.now() / 1000);
    const cleanupStmt = db
      .prepare("DELETE FROM consuming_get_receipts WHERE expires_at < ?")
      .bind(nowSeconds);
    const receiptStmt = db
      .prepare(
        `INSERT INTO consuming_get_receipts
           (requester_id, request_digest, recipient_id, target_id, expires_at)
         SELECT ?1, ?2, ?1, ?3, ?5
          WHERE EXISTS (
            SELECT 1 FROM users
             WHERE user_id = ?1 AND ik_ed25519_pub = ?4
          )`,
      )
      .bind(
        recipientId,
        requestDigest,
        contentId,
        expectedRecipientEd25519Pub,
        nowSeconds + 10 * 60,
      );
    const popStmt = db
      .prepare(
        `DELETE FROM wrapped_keys
          WHERE content_id = ?3
            AND recipient_id = ?1
            AND single_use = 1
            AND EXISTS (
              SELECT 1 FROM consuming_get_receipts
               WHERE requester_id = ?1
                 AND request_digest = ?2
                 AND recipient_id = ?1
                 AND target_id = ?3
            )
            AND EXISTS (
              SELECT 1 FROM users
               WHERE user_id = ?1 AND ik_ed25519_pub = ?4
            )
        RETURNING content_id, content_type, system_message_kind,
                  sender_id, recipient_id, session_version, share_index,
                  wrapped_share_blob, blob_version, single_use,
                  display_duration_seconds, expires_at, created_at`,
      )
      .bind(
        recipientId,
        requestDigest,
        contentId,
        expectedRecipientEd25519Pub,
      );
    let results: D1Result<WrappedKeyRow>[];
    try {
      results = await db.batch([cleanupStmt, receiptStmt, popStmt]);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (/UNIQUE|PRIMARY/i.test(msg)) {
        throw new ConsumingGetReplay("signed consuming GET already used");
      }
      throw err;
    }
    if ((results[1]?.meta?.changes ?? 0) !== 1) {
      return { status: "stale_identity" };
    }
    const popped = results[2]?.results?.[0] as WrappedKeyRow | undefined;
    if (!popped) {
      return (await identityKeyIsCurrent(
        db,
        recipientId,
        expectedRecipientEd25519Pub,
      ))
        ? { status: "not_found" }
        : { status: "stale_identity" };
    }
    if (expired) return { status: "gone" };
    return {
      status: "ok",
      row: { ...popped, single_use: true },
    };
  }
  if (!(await identityKeyIsCurrent(db, recipientId, expectedRecipientEd25519Pub))) {
    return { status: "stale_identity" };
  }
  if (expired) {
    const deleted = await db
      .prepare(
        `DELETE FROM wrapped_keys
          WHERE content_id = ?1
            AND recipient_id = ?2
            AND single_use = 0
            AND EXISTS (
              SELECT 1 FROM users
               WHERE user_id = ?2 AND ik_ed25519_pub = ?3
            )
        RETURNING content_id`,
      )
      .bind(contentId, recipientId, expectedRecipientEd25519Pub)
      .run();
    if ((deleted.results?.length ?? 0) === 1) return { status: "gone" };
    return (await identityKeyIsCurrent(
      db,
      recipientId,
      expectedRecipientEd25519Pub,
    ))
      ? { status: "not_found" }
      : { status: "stale_identity" };
  }
  return {
    status: "ok",
    row: { ...row, single_use: row.single_use === 1 },
  };
}

async function identityKeyIsCurrent(
  db: D1Database,
  userId: string,
  expectedEd25519Pub: string,
): Promise<boolean> {
  const row = await db
    .prepare(
      "SELECT 1 AS ok FROM users WHERE user_id = ? AND ik_ed25519_pub = ?",
    )
    .bind(userId, expectedEd25519Pub)
    .first<{ ok: number }>();
  return row?.ok === 1;
}

// ---- burn ----

export type BurnScopeStr = "single" | "to_user" | "all";

export type BurnWrappedKeysResult =
  | { status: "ok"; deleted_count: number }
  | { status: "replay" }
  | { status: "stale_identity" };

export async function burnWrappedKeysAuthenticated(
  db: D1Database,
  burningUserId: string,
  scope: BurnScopeStr,
  target: { content_id?: string; user_id?: string } | null,
  requestDigest: Uint8Array,
  expectedEd25519Pub: string,
  receiptExpiresAt: number,
): Promise<BurnWrappedKeysResult> {
  const cleanup = db
    .prepare("DELETE FROM wrapped_key_burn_receipts WHERE expires_at < ?")
    .bind(Math.floor(Date.now() / 1000));
  const receipt = db
    .prepare(
      `INSERT INTO wrapped_key_burn_receipts
         (user_id, signer_ed25519_pub, request_digest, expires_at)
       SELECT ?1, ?2, ?3, ?4
        WHERE EXISTS (
          SELECT 1 FROM users
           WHERE user_id = ?1 AND ik_ed25519_pub = ?2
        )`,
    )
    .bind(burningUserId, expectedEd25519Pub, requestDigest, receiptExpiresAt);
  let burn: D1PreparedStatement;
  if (scope === "single") {
    if (!target?.content_id) throw new Error("burn scope=single needs content_id");
    burn = db
      .prepare(
        `DELETE FROM wrapped_keys
          WHERE content_id = ?3 AND sender_id = ?1
            AND EXISTS (
              SELECT 1 FROM users
               WHERE user_id = ?1 AND ik_ed25519_pub = ?2
            )
            AND EXISTS (
              SELECT 1 FROM wrapped_key_burn_receipts
               WHERE user_id = ?1 AND signer_ed25519_pub = ?2
                 AND request_digest = ?4
            )
        RETURNING content_id`,
      )
      .bind(burningUserId, expectedEd25519Pub, target.content_id, requestDigest);
  } else if (scope === "to_user") {
    if (!target?.user_id) throw new Error("burn scope=to_user needs user_id");
    burn = db
      .prepare(
        `DELETE FROM wrapped_keys
          WHERE sender_id = ?1 AND recipient_id = ?3
            AND EXISTS (
              SELECT 1 FROM users
               WHERE user_id = ?1 AND ik_ed25519_pub = ?2
            )
            AND EXISTS (
              SELECT 1 FROM wrapped_key_burn_receipts
               WHERE user_id = ?1 AND signer_ed25519_pub = ?2
                 AND request_digest = ?4
            )
        RETURNING content_id`,
      )
      .bind(burningUserId, expectedEd25519Pub, target.user_id, requestDigest);
  } else {
    burn = db
      .prepare(
        `DELETE FROM wrapped_keys
          WHERE sender_id = ?1
            AND EXISTS (
              SELECT 1 FROM users
               WHERE user_id = ?1 AND ik_ed25519_pub = ?2
            )
            AND EXISTS (
              SELECT 1 FROM wrapped_key_burn_receipts
               WHERE user_id = ?1 AND signer_ed25519_pub = ?2
                 AND request_digest = ?3
            )
        RETURNING content_id`,
      )
      .bind(burningUserId, expectedEd25519Pub, requestDigest);
  }
  let results: D1Result[];
  try {
    results = await db.batch([cleanup, receipt, burn]);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (/wrapped_key_burn_receipts/i.test(message) && /UNIQUE|PRIMARY/i.test(message)) {
      return { status: "replay" };
    }
    throw err;
  }
  if ((results[1]?.meta?.changes ?? 0) !== 1) return { status: "stale_identity" };
  return { status: "ok", deleted_count: results[2]?.results?.length ?? 0 };
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

export type PrekeyReplenishResult =
  | "ok"
  | "replay"
  | "stale_identity"
  | "stale_spk"
  | "missing_spk";

/** Atomic, current-identity-bound SPK rotation + OPK append. */
export async function upsertPrekeyBundleAuthenticated(
  db: D1Database,
  userId: string,
  spk: SpkInput | null,
  opks: OpkInput[],
  requestDigest: Uint8Array,
  expectedEd25519Pub: string,
  receiptExpiresAt: number,
): Promise<PrekeyReplenishResult> {
  const statements: D1PreparedStatement[] = [
    db
      .prepare("DELETE FROM prekey_replenish_receipts WHERE expires_at < ?")
      .bind(Math.floor(Date.now() / 1000)),
    db
      .prepare(
        `INSERT INTO prekey_replenish_receipts
           (user_id, signer_ed25519_pub, request_digest, expires_at)
         SELECT ?1, ?2, ?3, ?4
          WHERE EXISTS (
            SELECT 1 FROM users
             WHERE user_id = ?1 AND ik_ed25519_pub = ?2
          )`,
      )
      .bind(userId, expectedEd25519Pub, requestDigest, receiptExpiresAt),
  ];
  if (spk) {
    statements.push(
      db
        .prepare(
          `UPDATE prekey_bundles
              SET prev_spk_pub = spk_pub,
                  prev_spk_signature = spk_signature,
                  prev_spk_rotated_at = spk_rotated_at,
                  spk_pub = ?4,
                  spk_signature = ?5,
                  spk_rotated_at = ?6
            WHERE user_id = ?1
              AND spk_rotated_at < ?6
              AND EXISTS (
                SELECT 1 FROM users
                 WHERE user_id = ?1 AND ik_ed25519_pub = ?2
              )
              AND EXISTS (
                SELECT 1 FROM prekey_replenish_receipts
                 WHERE user_id = ?1 AND signer_ed25519_pub = ?2
                   AND request_digest = ?3
                   AND expires_at = ?7
              )`,
        )
        .bind(
          userId,
          expectedEd25519Pub,
          requestDigest,
          spk.pub_b64,
          spk.signature_b64,
          spk.rotated_at,
          receiptExpiresAt,
        ),
      db
        .prepare(
          `INSERT INTO prekey_bundles
             (user_id, spk_pub, spk_signature, spk_rotated_at)
           SELECT ?1, ?4, ?5, ?6
            WHERE NOT EXISTS (
              SELECT 1 FROM prekey_bundles WHERE user_id = ?1
            )
              AND EXISTS (
                SELECT 1 FROM users
                 WHERE user_id = ?1 AND ik_ed25519_pub = ?2
              )
              AND EXISTS (
                SELECT 1 FROM prekey_replenish_receipts
                 WHERE user_id = ?1 AND signer_ed25519_pub = ?2
                   AND request_digest = ?3
                   AND expires_at = ?7
              )`,
        )
        .bind(
          userId,
          expectedEd25519Pub,
          requestDigest,
          spk.pub_b64,
          spk.signature_b64,
          spk.rotated_at,
          receiptExpiresAt,
        ),
    );
  }
  for (const o of opks) {
    statements.push(
      db
        .prepare(
          `INSERT INTO opk_pool (user_id, opk_id, opk_pub)
           SELECT ?1, ?4, ?5
            WHERE EXISTS (
              SELECT 1 FROM users
               WHERE user_id = ?1 AND ik_ed25519_pub = ?2
            )
              AND EXISTS (
                SELECT 1 FROM prekey_replenish_receipts
                 WHERE user_id = ?1 AND signer_ed25519_pub = ?2
                   AND request_digest = ?3
                   AND expires_at = ?6
              )
              AND EXISTS (
                SELECT 1 FROM prekey_bundles
                 WHERE user_id = ?1
                   AND (
                     ?7 = 0 OR (
                       spk_pub = ?8
                       AND spk_signature = ?9
                       AND spk_rotated_at = ?10
                     )
                   )
              )`,
        )
        .bind(
          userId,
          expectedEd25519Pub,
          requestDigest,
          o.id,
          o.pub_b64,
          receiptExpiresAt,
          spk ? 1 : 0,
          spk?.pub_b64 ?? "",
          spk?.signature_b64 ?? "",
          spk?.rotated_at ?? "",
        ),
    );
  }
  let results: D1Result[];
  try {
    results = await db.batch(statements);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    if (/prekey_replenish_receipts/i.test(msg) && /UNIQUE|PRIMARY/i.test(msg)) {
      return "replay";
    }
    if (/UNIQUE\s+constraint\s+failed/i.test(msg)) {
      throw new OpkIdConflict("opk id already used");
    }
    if (msg.includes("OPK pool quota exceeded")) {
      throw new OpkPoolQuotaExceeded("OPK pool quota exceeded");
    }
    throw err;
  }
  if ((results[1]?.meta?.changes ?? 0) !== 1) return "stale_identity";
  if (spk) {
    const spkChanges =
      (results[2]?.meta?.changes ?? 0) + (results[3]?.meta?.changes ?? 0);
    if (spkChanges !== 1) {
      const current = await db
        .prepare(
          `SELECT 1 AS ok FROM prekey_bundles
            WHERE user_id = ? AND spk_pub = ?
              AND spk_signature = ? AND spk_rotated_at = ?`,
        )
        .bind(userId, spk.pub_b64, spk.signature_b64, spk.rotated_at)
        .first<{ ok: number }>();
      if (current?.ok !== 1) return "stale_spk";
    }
  }
  const firstOpkResult = spk ? 4 : 2;
  for (let i = 0; i < opks.length; i += 1) {
    if ((results[firstOpkResult + i]?.meta?.changes ?? 0) !== 1) {
      if (!spk) {
        const bundle = await db
          .prepare("SELECT 1 AS ok FROM prekey_bundles WHERE user_id = ?")
          .bind(userId)
          .first<{ ok: number }>();
        if (bundle?.ok !== 1) return "missing_spk";
      }
      return "stale_spk";
    }
  }
  return "ok";
}

export class OpkIdConflict extends Error {}
export class OpkPoolQuotaExceeded extends Error {}

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

export class ConsumingGetReplay extends Error {}

export type PopPrekeyBundleResult =
  | { status: "ok"; bundle: PrekeyBundleResponse }
  | { status: "not_found" }
  | { status: "stale_identity" };

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
 * Returns `not_found` when the user has no identity row OR no SPK row.
 * `opk = null` means the pool was empty (design-doc OPK-exhaustion
 * fallback — senders skip DH4 and proceed with PQXDH).
 */
export async function popPrekeyBundleAuthenticated(
  db: D1Database,
  recipientId: string,
  requesterId: string,
  requestDigest: Uint8Array,
  expectedRequesterEd25519Pub: string,
): Promise<PopPrekeyBundleResult> {
  // Identity + SPK are read up front; they don't mutate during the
  // pop so they don't need to be inside the batch.
  const [userRow, spkRow] = await Promise.all([
    db
      .prepare(
        `SELECT user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
                ik_ratchet_initial_pub
           FROM users WHERE user_id = ?`,
      )
      .bind(recipientId)
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
      .bind(recipientId)
      .first<{ spk_pub: string; spk_signature: string; spk_rotated_at: string }>(),
  ]);
  if (!userRow || !spkRow) return { status: "not_found" };

  // The receipt insert and OPK pop are one D1 transaction. An exact
  // replay collides on the receipt PK and aborts the batch before the
  // delete. The insert is also conditional on the requester's verified
  // identity key still being current (rotation-safe CAS).
  const nowSeconds = Math.floor(Date.now() / 1000);
  const receiptExpiry = nowSeconds + 10 * 60;
  const cleanupStmt = db
    .prepare("DELETE FROM consuming_get_receipts WHERE expires_at < ?")
    .bind(nowSeconds);
  const receiptStmt = db
    .prepare(
      `INSERT INTO consuming_get_receipts
         (requester_id, request_digest, recipient_id, target_id, expires_at)
       SELECT ?1, ?2, ?3, ?3, ?5
        WHERE EXISTS (
          SELECT 1 FROM users
           WHERE user_id = ?1 AND ik_ed25519_pub = ?4
        )`,
    )
    .bind(
      requesterId,
      requestDigest,
      recipientId,
      expectedRequesterEd25519Pub,
      receiptExpiry,
    );
  const popStmt = db
    .prepare(
      `DELETE FROM opk_pool
        WHERE user_id = ?3
          AND opk_id = (SELECT MIN(opk_id) FROM opk_pool WHERE user_id = ?3)
          AND EXISTS (
            SELECT 1 FROM consuming_get_receipts
             WHERE requester_id = ?1
               AND request_digest = ?2
               AND recipient_id = ?3
               AND target_id = ?3
          )
          AND EXISTS (
            SELECT 1 FROM users
             WHERE user_id = ?1 AND ik_ed25519_pub = ?4
          )
        RETURNING opk_id, opk_pub`,
    )
    .bind(
      requesterId,
      requestDigest,
      recipientId,
      expectedRequesterEd25519Pub,
    );
  const countStmt = db
    .prepare("SELECT COUNT(*) AS c FROM opk_pool WHERE user_id = ?")
    .bind(recipientId);
  let batchResults: D1Result<{
    opk_id?: number;
    opk_pub?: string;
    c?: number;
  }>[];
  try {
    batchResults = await db.batch([
      cleanupStmt,
      receiptStmt,
      popStmt,
      countStmt,
    ]);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    if (/UNIQUE|PRIMARY/i.test(msg)) {
      throw new ConsumingGetReplay("signed consuming GET already used");
    }
    throw err;
  }
  const receiptRes = batchResults[1];
  if ((receiptRes?.meta?.changes ?? 0) !== 1) {
    return { status: "stale_identity" };
  }
  const popRes = batchResults[2];
  const countRes = batchResults[3];
  const popped = popRes?.results?.[0] as { opk_id: number; opk_pub: string } | undefined;
  const remaining = ((countRes?.results?.[0] as { c: number } | undefined)?.c) ?? 0;
  const consumedOpk = popped
    ? { id: popped.opk_id, pub_b64: popped.opk_pub }
    : null;

  return {
    status: "ok",
    bundle: {
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
    },
  };
}
