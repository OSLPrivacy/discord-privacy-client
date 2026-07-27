/// Phase 6.4: SKDM inbox endpoints.
///
///   POST   /v1/skdm/inbox          enqueue an SKDM bundle for a recipient
///   GET    /v1/skdm/inbox/:user_id drain the user's own inbox (FIFO)
///   DELETE /v1/skdm/inbox/:id      delete a specific row after apply
///
/// All three are authorized by an ed25519 signature over canonical
/// bytes (see ../lib/canonical.ts). Signing key is the requester's
/// `ik_ed25519_pub` row in the existing `users` table -- one less
/// auth surface to maintain.
///
/// The bundle blob is opaque to this server; it's the same v=3
/// multi-recipient PQ-hybrid wire encrypt_v5_send already produces.
///
/// TTL: CONTROL_INBOX_TTL_SECONDS (7 days). The existing keyserver
/// cron sweep deletes rows where expires_at < now (see ./sweep
/// caller; we just rely on the standard sweep job picking up the
/// expires_at index).

import type { Env } from "../env.js";
import {
  canonicalControlInboxDeleteBytes,
  canonicalControlInboxGetBytes,
  canonicalControlInboxPostBytes,
  CONTROL_INBOX_FRESHNESS_WINDOW_MS,
} from "../lib/canonical.js";
import { verifyEd25519 } from "../lib/crypto.js";
import {
  controlInboxDispositionSchemaReady,
} from "../lib/control-inbox-sweep.js";
import { getUserForVerify } from "../lib/db.js";
import {
  badRequest,
  json,
  notFound,
  recipientInboxFull,
  revocationLaneFull,
  serverError,
  serviceUnavailable,
  tooMany,
  unauthorized,
} from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import {
  decodeBase64,
  isNonEmptyBase64,
  isProtocolId,
} from "../lib/validation.js";

const CONTROL_INBOX_TTL_SECONDS = 7 * 24 * 60 * 60;
const MAX_BUNDLE_BYTES = 16 * 1024;
const MAX_DRAIN_ROWS = 64;
// At the maximum 16 KiB payload this bounds one recipient's physical ordinary
// lane to 8 MiB. Retained retry/quarantine rows count toward the cap: excluding
// them would create an unbounded second quota behind the live-delivery quota.
// Applied/deleted messages stop counting, while replay receipts remain until
// their normal seven-day expiry.
const MAX_PENDING_ROWS_PER_RECIPIENT = 512;
const MAX_PENDING_ROWS_PER_SENDER_RECIPIENT = 32;

/// Slots inside `MAX_PENDING_ROWS_PER_RECIPIENT` that a sender who already
/// holds a meaningful backlog may not consume.
///
/// Registration is open, so 16 identities holding 32 rows each used to be
/// enough to reach the recipient-wide cap. The old answer was to delete the
/// oldest undelivered rows for that recipient no matter who sent them, which
/// let an attacker silently destroy an unrelated sender's pending control state
/// (2026-07-26 audit). Refusing instead is correct but, on its own, converts
/// that attack into "nobody can reach this recipient at all". The reserve keeps
/// a first contact deliverable through a congested lane: 128 slots at up to
/// four rows apiece is 32 distinct senders who can still get a message in after
/// the ordinary allowance is gone.
const RESERVED_FRESH_SENDER_ROWS = 128;

/// How much a sender may already hold and still draw on that reserve.
const FRESH_SENDER_ROWS = 4;

/// The two lanes. `""` is the ordinary, evictable lane every existing client
/// posts to; `revocation` is the bilateral-burn lane.
const KIND_ORDINARY = "";
const KIND_REVOCATION = "revocation";
const KNOWN_KINDS = new Set([KIND_ORDINARY, KIND_REVOCATION]);
/// Revocation-lane caps. Mirrored by the triggers in migration 0027, which are
/// the race-safe backstop for these pre-checks.
///
/// Small on purpose: a burn is one notice per conversation per epoch, and the
/// lane collapses a repeat for the same (scope, epoch), so eight outstanding
/// burns from one peer is already generous. Reaching either cap answers 507 --
/// never an eviction, because a silently deleted burn is the one failure mode
/// this whole lane exists to remove.
const MAX_PENDING_REVOCATIONS_PER_RECIPIENT = 64;
const MAX_PENDING_REVOCATIONS_PER_SENDER_RECIPIENT = 8;

/// Opaque collapse key: 64 lowercase hex characters. Computed by the client as a
/// MAC over (scope commitment, burn epoch) under a key derived from the two
/// identity public keys, so this server learns neither the scope nor the epoch
/// and cannot link one pair's lane to another's.
const COLLAPSE_KEY_RE = /^[0-9a-f]{64}$/;

const INBOX_ID_BYTES = 16;
const INBOX_ID_HEX_LEN = INBOX_ID_BYTES * 2;
const INBOX_ID_HEX_RE = /^[0-9a-f]{32}$/;

function genInboxId(): Uint8Array {
  const buf = new Uint8Array(INBOX_ID_BYTES);
  crypto.getRandomValues(buf);
  return buf;
}

function idToHex(id: Uint8Array): string {
  let hex = "";
  for (const b of id) hex += b.toString(16).padStart(2, "0");
  return hex;
}

function hexToId(hex: string): Uint8Array | null {
  if (hex.length !== INBOX_ID_HEX_LEN) return null;
  if (!INBOX_ID_HEX_RE.test(hex)) return null;
  const out = new Uint8Array(INBOX_ID_BYTES);
  for (let i = 0; i < INBOX_ID_BYTES; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function freshnessOk(ts: unknown): ts is number {
  if (typeof ts !== "number" || !Number.isFinite(ts) || ts <= 0) return false;
  return Math.abs(Date.now() - ts) <= CONTROL_INBOX_FRESHNESS_WINDOW_MS;
}

/// Decode base64 WITHOUT throwing. `decodeBase64` calls `atob`, which
/// throws `InvalidCharacterError` on a malformed / null / undefined
/// value; an unguarded call turned a missing-or-bad signing key into
/// an opaque HTTP 500 ("internal error") that broke the whole drain
/// loop. Returns null on any decode failure so callers can answer
/// with a clean 401/400 instead.
function safeDecodeBase64(value: unknown): Uint8Array | null {
  if (typeof value !== "string" || value.length === 0) return null;
  try {
    return decodeBase64(value);
  } catch {
    return null;
  }
}

function bytesToU8(v: unknown): Uint8Array | null {
  if (v == null) return null;
  if (v instanceof Uint8Array) return v;
  if (v instanceof ArrayBuffer) return new Uint8Array(v);
  if (ArrayBuffer.isView(v)) {
    const view = v as ArrayBufferView;
    return new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  }
  // D1 returns BLOB columns as a plain Array<number> (byte values).
  // THIS was the GC bug: an unhandled array fell through to `null` →
  // the bundle/id came back EMPTY → the GET shipped bundle_b64="" → the
  // client decoded 0 bytes → "wire too short: 0 bytes" on every SKDM.
  if (Array.isArray(v)) {
    return Uint8Array.from(v as number[]);
  }
  // Defensive: some runtimes hand back an object with numeric keys
  // ({0:.., 1:.., length}) or a {data:[...]} wrapper.
  if (typeof v === "object") {
    const o = v as Record<string, unknown>;
    if (Array.isArray(o.data)) return Uint8Array.from(o.data as number[]);
    const keys = Object.keys(o).filter((k) => /^\d+$/.test(k));
    if (keys.length > 0) {
      const arr = new Uint8Array(keys.length);
      for (const k of keys) arr[Number(k)] = Number(o[k]);
      return arr;
    }
  }
  if (typeof v === "string") {
    try {
      const bin = atob(v);
      const arr = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
      return arr;
    } catch {
      return null;
    }
  }
  return null;
}

async function sha256(bytes: Uint8Array): Promise<Uint8Array> {
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  return new Uint8Array(hash);
}

async function findRequestReceipt(
  db: D1Database,
  senderId: string,
  requestDigest: Uint8Array,
): Promise<{ id: string; expires_at: number } | null> {
  const row = await db
    .prepare(
      `SELECT inbox_id, expires_at
         FROM control_inbox_requests
        WHERE sender_id = ? AND request_digest = ?`,
    )
    .bind(senderId, requestDigest)
    .first<{ inbox_id: unknown; expires_at: number }>();
  if (!row) return null;
  const id = bytesToU8(row.inbox_id);
  if (!id || id.length !== INBOX_ID_BYTES) {
    throw new Error("invalid inbox id in request receipt");
  }
  return { id: idToHex(id), expires_at: row.expires_at };
}

/**
 * Drop the oldest live row owned by the posting sender so a new one fits.
 *
 * Retained retry/quarantine rows are never candidates. If they occupy the
 * physical pair cap, the D1 trigger refuses the insert explicitly instead.
 *
 * INVARIANT (2026-07-26 audit fix): `whereClause` must always be scoped to the
 * posting sender. Eviction is now only ever a sender recycling its OWN stalest
 * row. A recipient-wide predicate here is what let any registered sender delete
 * an unrelated sender's undelivered control state; the recipient-wide cap is
 * enforced by refusal in `handleControlInboxPost`, never by deletion.
 */
async function evictOldestPending(
  env: Env,
  whereClause: string,
  binds: string[],
  count: number,
  limit: number,
): Promise<number> {
  const excess = count - limit + 1;
  if (excess <= 0) return 0;
  const result = await env.DB
    .prepare(
      `DELETE FROM control_inbox
        WHERE id IN (
          SELECT id FROM control_inbox
           WHERE ${whereClause}
             AND delivery_status = 'live'
           ORDER BY created_at ASC
           LIMIT ?
        )`,
    )
    .bind(...binds, excess)
    .run();
  return result.meta?.changes ?? 0;
}

export async function handleControlInboxPost(
  request: Request,
  env: Env,
): Promise<Response> {
  // Generous-ish: peers can post a lot of SKDMs in a busy session
  // (SKDM fan-out posts once per recipient). Own bucket so the
  // drain loop's GET/DELETE traffic can't starve sends.
  const rl = await checkRateLimit(env, callerIp(request), 1200, "ci-post");
  if (!rl.ok) return tooMany(rl.retryAfter);
  if (!(await controlInboxDispositionSchemaReady(env.DB))) {
    return serviceUnavailable("control inbox schema unavailable");
  }

  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }

  if (!isProtocolId(body.sender_id)) return badRequest("sender_id must be a bounded identifier");
  if (!isProtocolId(body.recipient_id))
    return badRequest("recipient_id must be a bounded identifier");
  if (!isProtocolId(body.scope_id)) return badRequest("scope_id must be a bounded identifier");
  if (!isNonEmptyBase64(body.bundle_b64))
    return badRequest("bundle_b64 required (non-empty base64)");
  if (!isNonEmptyBase64(body.signature_b64))
    return badRequest("signature_b64 required");
  if (!freshnessOk(body.timestamp_ms))
    return badRequest(
      `timestamp_ms required (positive number within ${CONTROL_INBOX_FRESHNESS_WINDOW_MS}ms of server clock)`,
    );

  // Lane selection. Absent is the ordinary lane, which is what every deployed
  // client sends and what keeps their canonical bytes byte-identical.
  //
  // FAIL CLOSED on anything unrecognised rather than defaulting to a lane: a
  // typo that quietly landed a burn in the evictable lane would be the exact
  // silent loss this lane exists to prevent, and one that quietly landed
  // ordinary traffic in the non-evictable lane would let a peer exhaust it.
  if (body.kind !== undefined && typeof body.kind !== "string") {
    return badRequest("kind must be a string when present");
  }
  const kind: string = typeof body.kind === "string" ? body.kind : KIND_ORDINARY;
  if (!KNOWN_KINDS.has(kind)) return badRequest("kind is not recognised");
  const isRevocation = kind === KIND_REVOCATION;

  // The collapse key is required for a revocation and forbidden otherwise. Both
  // halves matter: without it the lane cannot collapse a retry, and allowing it
  // on an ordinary row would let a caller move that row out of the evictable set.
  if (body.collapse_key !== undefined && typeof body.collapse_key !== "string") {
    return badRequest("collapse_key must be a string when present");
  }
  const collapseKey: string | null =
    typeof body.collapse_key === "string" ? body.collapse_key : null;
  if (isRevocation) {
    if (collapseKey === null) {
      return badRequest("collapse_key required for a revocation");
    }
    if (!COLLAPSE_KEY_RE.test(collapseKey)) {
      return badRequest("collapse_key must be 64 lowercase hex characters");
    }
  } else if (collapseKey !== null) {
    return badRequest("collapse_key is only valid for a revocation");
  }

  const bundle = safeDecodeBase64(body.bundle_b64);
  if (!bundle) return badRequest("bundle_b64 must be valid base64");
  if (bundle.length === 0) return badRequest("bundle is empty");
  if (bundle.length > MAX_BUNDLE_BYTES)
    return badRequest(`bundle exceeds ${MAX_BUNDLE_BYTES} bytes`);

  const sender = await getUserForVerify(env.DB, body.sender_id);
  if (!sender) return notFound();

  const bundleHash = await sha256(bundle);
  const message = canonicalControlInboxPostBytes({
    sender_id: body.sender_id,
    recipient_id: body.recipient_id,
    scope_id: body.scope_id,
    timestamp_ms: body.timestamp_ms,
    bundle_sha256: bundleHash,
    // Both are signed components. An `undefined` here reproduces the pre-lane
    // bytes exactly, so an old client's signature still verifies; a request that
    // adds, removes or alters either in transit stops verifying (401).
    kind: kind === KIND_ORDINARY ? undefined : kind,
    collapse_key: collapseKey ?? undefined,
  });
  const pubBytes = safeDecodeBase64(sender.ik_ed25519_pub);
  const sigBytes = safeDecodeBase64(body.signature_b64);
  if (!pubBytes) {
    return unauthorized("no usable ed25519 signing key on file for sender");
  }
  if (!sigBytes) return badRequest("signature is not valid base64");
  const ok = await verifyEd25519(pubBytes, message, sigBytes);
  if (!ok) return unauthorized("signature verification failed");

  // Hash the exact canonical bytes that were just verified. A receipt
  // survives deletion of the inbox item, preventing a captured request
  // from re-enqueueing after the recipient applies it.
  const requestDigest = await sha256(message);
  const prior = await findRequestReceipt(
    env.DB,
    body.sender_id,
    requestDigest,
  );
  if (prior) return json({ ...prior, replayed: true }, { status: 200 });

  // Do not accept opaque storage for arbitrary/nonexistent recipient
  // identifiers. Perform this only after sender authentication so the
  // route is not an unauthenticated registration oracle.
  const recipient = await getUserForVerify(env.DB, body.recipient_id);
  if (!recipient) return notFound();

  const now = Math.floor(Date.now() / 1000);

  // ---- The revocation lane -------------------------------------------------
  //
  // Separate from everything below. Counts are physical, not just live: held
  // disposition rows may not create a second unbounded revocation quota.
  // Refuse when full; never evict.
  if (isRevocation) {
    const laneForRecipient = await env.DB
      .prepare(
        `SELECT COUNT(*) AS count
          FROM control_inbox
          WHERE recipient_id = ? AND kind = ?`,
      )
      .bind(body.recipient_id, KIND_REVOCATION)
      .first<{ count: number }>();
    const laneForPair = await env.DB
      .prepare(
        `SELECT COUNT(*) AS count
          FROM control_inbox
          WHERE recipient_id = ? AND sender_id = ? AND kind = ?`,
      )
      .bind(body.recipient_id, body.sender_id, KIND_REVOCATION)
      .first<{ count: number }>();
    // An existing row for the same (recipient, sender, scope, collapse_key)
    // is going to be UPSERTed, not appended, so it does not need lane headroom.
    const collapsible = await env.DB
      .prepare(
        `SELECT COUNT(*) AS count
           FROM control_inbox
          WHERE recipient_id = ? AND sender_id = ? AND scope_id = ?
            AND collapse_key = ?`,
      )
      .bind(body.recipient_id, body.sender_id, body.scope_id, collapseKey)
      .first<{ count: number }>();
    const willCollapse = (collapsible?.count ?? 0) > 0;
    if (!willCollapse) {
      if ((laneForPair?.count ?? 0) >= MAX_PENDING_REVOCATIONS_PER_SENDER_RECIPIENT) {
        return revocationLaneFull(60, "sender_recipient");
      }
      if ((laneForRecipient?.count ?? 0) >= MAX_PENDING_REVOCATIONS_PER_RECIPIENT) {
        return revocationLaneFull(60, "recipient");
      }
    }
    return await insertControlInboxRow(env, {
      recipientId: body.recipient_id,
      senderId: body.sender_id,
      scopeId: body.scope_id,
      bundle,
      kind: KIND_REVOCATION,
      collapseKey,
      senderSigningKey: sender.ik_ed25519_pub,
      requestDigest,
      now,
    });
  }

  // ---- Ordinary lane admission ---------------------------------------------
  //
  // Two caps, two different answers, and the difference is the audit fix.
  //
  // PER PAIR — evict. A sender's 33rd undelivered message to one person
  // displaces that same sender's own stalest one. Refusing here would punish
  // the sender for something only the recipient can fix: these rows are drained
  // by the recipient, not by a timer, so a recipient who never runs OSL would
  // block the sender for the full seven-day TTL. Recycling your own slot is
  // safe because the loss falls on the party who chose to keep sending.
  //
  // RECIPIENT-WIDE — refuse. This cap used to be enforced the same way, by
  // deleting the recipient's oldest undelivered rows regardless of sender.
  // Since registration is open, that let an attacker with a few identities
  // silently destroy an unrelated sender's pending SKDM/control state: the
  // victim's dependent protected messages then became unopenable, with no error
  // to either side. Cross-sender data loss is strictly worse than a sender
  // learning it must retry, so the answer is explicit backpressure.
  //
  // `kind = ''` scopes both to the ordinary lane. Counts include retained and
  // expired physical rows; the sweep reclaims those under the disposition
  // policy, while POST must backpressure rather than bypass the storage bound.
  // A revocation row must never
  // be a victim -- that is the defect the separate lane exists to fix -- and
  // excluding revocations from the count keeps a queued burn from consuming an
  // ordinary conversation's headroom.
  const pendingFromSenderBefore = await env.DB
    .prepare(
      `SELECT COUNT(*) AS count
         FROM control_inbox
        WHERE recipient_id = ? AND sender_id = ? AND kind = ?`,
    )
    .bind(body.recipient_id, body.sender_id, KIND_ORDINARY)
    .first<{ count: number }>();
  const pending = await env.DB
    .prepare(
      `SELECT COUNT(*) AS count
         FROM control_inbox
        WHERE recipient_id = ? AND kind = ?`,
    )
    .bind(body.recipient_id, KIND_ORDINARY)
    .first<{ count: number }>();

  // DECIDE BEFORE MUTATING.
  //
  // Eviction used to run here, before the recipient-wide check. That made the
  // refusal path destructive: a sender at its 32-row pair cap posting to a
  // congested recipient had one of its OWN queued rows deleted to make room,
  // and was then refused anyway — losing undelivered control state while being
  // told the send had failed. Fixing cross-sender deletion only to introduce
  // same-sender deletion on a failure path is not a fix.
  //
  // So the outcome is computed from what eviction *would* free, and nothing is
  // deleted unless the row is actually going to be inserted.
  const heldBySender = pendingFromSenderBefore?.count ?? 0;
  const wouldRecycle = Math.max(
    0,
    heldBySender - MAX_PENDING_ROWS_PER_SENDER_RECIPIENT + 1,
  );
  // A sender still holding a real backlog may only use the ordinary allowance;
  // the reserve is there so congestion caused by others cannot make a first
  // contact undeliverable.
  const admissionCap = heldBySender - wouldRecycle < FRESH_SENDER_ROWS
    ? MAX_PENDING_ROWS_PER_RECIPIENT
    : MAX_PENDING_ROWS_PER_RECIPIENT - RESERVED_FRESH_SENDER_ROWS;
  if ((pending?.count ?? 0) - wouldRecycle >= admissionCap) {
    return recipientInboxFull(60, "recipient");
  }

  // Admitted. Only now may this sender recycle its own stalest rows.
  await evictOldestPending(
    env,
    `recipient_id = ? AND sender_id = ? AND kind = ''`,
    [body.recipient_id, body.sender_id],
    heldBySender,
    MAX_PENDING_ROWS_PER_SENDER_RECIPIENT,
  );
  // The check above is a pre-check, not the enforcement. Two concurrent posts
  // could both read a count under the cap and both insert. Migration 0031's
  // replacement trigger backstops the hard physical 512, but it knows nothing
  // about the reserve, so
  // the cap is carried into the insert statement itself as well -- same lesson
  // as the cipher-store limiter: a check separated from its act is not a bound.
  return await insertControlInboxRow(env, {
    admissionCap,
    recipientId: body.recipient_id,
    senderId: body.sender_id,
    scopeId: body.scope_id,
    bundle,
    kind: KIND_ORDINARY,
    collapseKey: null,
    senderSigningKey: sender.ik_ed25519_pub,
    requestDigest,
    now,
  });
}

/**
 * The authenticated insert, shared by both lanes.
 *
 * Split out of `handleControlInboxPost` so the revocation lane reuses the exact
 * same durability, identity-rebinding and idempotency-receipt logic rather than
 * a parallel copy that could drift from it.
 *
 * A revocation row additionally UPSERTs on the partial unique index from
 * migration 0027: a second burn for the same `(recipient, sender, scope,
 * collapse_key)` replaces the queued bundle instead of appending. The collapse
 * key is derived by the client from (scope commitment, burn epoch), so "same
 * scope, same epoch" collapses and a genuinely new epoch does not. That is what
 * keeps a retried burn from filling the lane, and it is safe because the newer
 * bundle supersedes the older one by construction -- both assert the same epoch.
 */
async function insertControlInboxRow(
  env: Env,
  args: {
    recipientId: string;
    senderId: string;
    scopeId: string;
    bundle: Uint8Array;
    kind: string;
    collapseKey: string | null;
    senderSigningKey: string;
    requestDigest: Uint8Array;
    now: number;
    /**
     * Recipient-wide ordinary-lane ceiling, enforced inside the insert so the
     * caller's pre-check cannot be raced past. `null` for the revocation lane,
     * which has its own triggers and must never be refused by this rule.
     */
    admissionCap?: number | null;
  },
): Promise<Response> {
  const { now } = args;
  // Insert. Retry on the (vanishingly unlikely) primary-key
  // collision; 128-bit random id space means it's basically never.
  const expiresAt = now + CONTROL_INBOX_TTL_SECONDS;
  const receiptExpiresAt =
    now + Math.ceil((2 * CONTROL_INBOX_FRESHNESS_WINDOW_MS) / 1000);
  const isRevocation = args.kind === KIND_REVOCATION;
  for (let attempt = 0; attempt < 5; attempt++) {
    const id = genInboxId();
    try {
      const rowSql =
        `INSERT INTO control_inbox
           (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at, kind, collapse_key)
         SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?9, ?10
          WHERE EXISTS (
            SELECT 1 FROM users
             WHERE user_id = ?3 AND ik_ed25519_pub = ?8
          )
            AND EXISTS (
              SELECT 1 FROM users WHERE user_id = ?2
            )
            AND (?11 IS NULL OR (
                SELECT COUNT(*) FROM control_inbox
                   WHERE recipient_id = ?2 AND kind = ''
                ) < ?11)` +
        (isRevocation
          ? `
         ON CONFLICT (recipient_id, sender_id, scope_id, collapse_key)
           WHERE collapse_key IS NOT NULL
           DO UPDATE SET bundle = excluded.bundle,
                         expires_at = excluded.expires_at,
                         created_at = excluded.created_at,
                         delivery_status = 'live',
                         delivery_reason = NULL,
                         delivery_attempts = 0,
                         sender_disabled_first_seen_at = NULL,
                         delivery_next_retry_at = NULL,
                         delivery_retain_until = NULL`
          : "");
      const results = await env.DB.batch([
        env.DB
          .prepare(rowSql)
          .bind(
            id,
            args.recipientId,
            args.senderId,
            args.scopeId,
            args.bundle,
            expiresAt,
            now,
            args.senderSigningKey,
            args.kind,
            args.collapseKey,
            args.admissionCap ?? null,
          ),
        env.DB
          .prepare(
            `INSERT INTO control_inbox_requests
               (sender_id, request_digest, inbox_id, recipient_id, expires_at)
             SELECT ?1, ?2, ?3, ?4, ?5
              WHERE EXISTS (
                SELECT 1 FROM users
                 WHERE user_id = ?1 AND ik_ed25519_pub = ?6
              )
                AND EXISTS (
                  SELECT 1 FROM control_inbox
                   WHERE sender_id = ?1 AND recipient_id = ?4
                     AND (id = ?3 OR ?7 = 1)
                )`,
          )
          .bind(
            args.senderId,
            args.requestDigest,
            id,
            args.recipientId,
            receiptExpiresAt,
            args.senderSigningKey,
            isRevocation ? 1 : 0,
          ),
      ]);
      if (
        (results[0]?.meta?.changes ?? 0) !== 1 ||
        (results[1]?.meta?.changes ?? 0) !== 1
      ) {
        const currentSender = await getUserForVerify(env.DB, args.senderId);
        if (currentSender?.ik_ed25519_pub !== args.senderSigningKey) {
          return unauthorized("sender identity changed during authorization");
        }
        if (!(await getUserForVerify(env.DB, args.recipientId))) return notFound();
        // The admission predicate is the remaining reason the statement can
        // legitimately affect nothing: a concurrent post filled the lane between
        // the caller's pre-check and this insert. Same durable condition, so the
        // same answer rather than an opaque 500.
        if (args.admissionCap != null) {
          const raced = await env.DB
            .prepare(
              `SELECT COUNT(*) AS count FROM control_inbox
                WHERE recipient_id = ? AND kind = ''`,
            )
            .bind(args.recipientId)
            .first<{ count: number }>();
          if ((raced?.count ?? 0) >= args.admissionCap) {
            return recipientInboxFull(60, "recipient");
          }
        }
        throw new Error("control inbox authenticated insert made no change");
      }
      // On a collapse the caller's row id is not the surviving one, so report the
      // id that is actually in the table. A client addresses rows by drain, not
      // by this id, but returning a phantom id would make the response a lie.
      let reportedId = idToHex(id);
      if (isRevocation) {
        const surviving = await env.DB
          .prepare(
            `SELECT id FROM control_inbox
              WHERE recipient_id = ? AND sender_id = ? AND scope_id = ?
                AND collapse_key = ?`,
          )
          .bind(args.recipientId, args.senderId, args.scopeId, args.collapseKey)
          .first<{ id: unknown }>();
        const survivingId = bytesToU8(surviving?.id);
        if (survivingId && survivingId.length === INBOX_ID_BYTES) {
          reportedId = idToHex(survivingId);
        }
      }
      return json({ id: reportedId, expires_at: expiresAt }, { status: 201 });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      // The D1 triggers (migrations 0006 / 0016 / 0027) are the race-safe
      // backstop for the SELECT-based checks in the caller. Same durable
      // condition, so report the same code -- and carry the same scope
      // distinction the pre-checks do rather than collapsing both into one label.
      if (msg.includes("control inbox revocation lane quota exceeded")) {
        return revocationLaneFull(60, "recipient");
      }
      if (msg.includes("control inbox revocation sender lane quota exceeded")) {
        return revocationLaneFull(60, "sender_recipient");
      }
      if (msg.includes("control inbox recipient quota exceeded")) {
        return recipientInboxFull(60, "recipient");
      }
      if (msg.includes("control inbox sender-recipient quota exceeded")) {
        return recipientInboxFull(60, "sender_recipient");
      }
      if (
        msg.includes("control inbox kind is not recognised") ||
        msg.includes("control inbox collapse key does not match kind")
      ) {
        return badRequest("control inbox lane request is invalid");
      }
      if (msg.includes("UNIQUE") || msg.includes("PRIMARY")) {
        // Either a simultaneous retry won the digest CAS, or the
        // random inbox id collided. Distinguish them by reading the
        // durable receipt; only the latter should allocate a new id.
        const raced = await findRequestReceipt(
          env.DB,
          args.senderId,
          args.requestDigest,
        );
        if (raced) {
          return json({ ...raced, replayed: true }, { status: 200 });
        }
        continue;
      }
      throw err;
    }
  }
  return badRequest("could not allocate a fresh id (retry)");
}

/**
 * `GET /v1/control-inbox/:user_id[?sender=<id>]`
 *
 * # The bug the `sender` filter fixes
 *
 * This route returns at most `MAX_DRAIN_ROWS` (64) rows,
 * `ORDER BY created_at ASC`, with no cursor. The Hub's drain
 * authenticates each row against the conversation it currently has
 * open and **deletes only the rows it authenticates** — rows belonging
 * to other peers are skipped and left in place.
 *
 * Those two facts compose into permanent, invisible undeliverability:
 * once 64 older rows addressed from peers whose conversations are not
 * open exist, every row for the *active* conversation sits past the page
 * boundary and is never returned. The drain then sees the same empty
 * result as a genuinely empty inbox, so there is no error and no log.
 *
 * No attacker is needed. This route admits 32 pending rows per
 * (sender, recipient) pair and 512 per recipient, so **two** friends
 * with unopened conversations are enough. The stuck rows clear only on
 * TTL expiry (7 days) or 512-ceiling eviction.
 *
 * # Why a filter rather than a cursor
 *
 * A cursor would let a client walk past the blockage, but it needs
 * per-peer client state, it costs a round trip per page, and it only
 * bounds the damage — a client that gives up walking after N pages is
 * starved again. The filter removes the coupling entirely: delivery for
 * one conversation stops depending on any other peer's backlog.
 *
 * It is also *complete* here, which a cursor is not. The per-pair
 * admission cap is `MAX_PENDING_ROWS_PER_SENDER_RECIPIENT` = 32, well
 * under the 64-row page, so a filtered drain for one sender can never
 * be truncated by the page limit at all. There is nothing left to
 * paginate.
 *
 * The unfiltered form is unchanged and still supported: a deployed
 * worker serves older clients that do not know about the parameter.
 */
export async function handleControlInboxGet(
  request: Request,
  env: Env,
  userId: string,
): Promise<Response> {
  if (!(await controlInboxDispositionSchemaReady(env.DB))) {
    return serviceUnavailable("control inbox schema unavailable");
  }
  try {
    return await handleControlInboxGetInner(request, env, userId);
  } catch {
    console.error("[control-inbox GET] failed");
    return serverError("internal error");
  }
}

async function handleControlInboxGetInner(
  request: Request,
  env: Env,
  userId: string,
): Promise<Response> {
  // NO rate-limit KV write here. The drain polls this GET every ~10s
  // (8,640×/day per client), and the per-request KV write that the
  // throttle does was blowing the free-tier KV write quota (1,000/day
  // per account) — which then 500'd EVERY KV-touching endpoint,
  // including cipher-store uploads. The GET is already authenticated
  // by an ed25519 signature over a fresh timestamp and is read-only
  // (a D1 SELECT), so a throttle adds little and isn't worth a KV
  // write per poll. POST/DELETE keep their throttles.

  const url = new URL(request.url);
  const ts = parseInt(url.searchParams.get("ts") || "", 10);
  const sigB64 = url.searchParams.get("sig") || "";
  if (!freshnessOk(ts)) return badRequest("ts required (?ts= within window)");
  if (!sigB64) return badRequest("sig required (?sig=)");
  if (!isProtocolId(userId)) return badRequest("user_id must be a bounded identifier");

  // Optional per-sender filter. See the "head-of-line starvation" note
  // on `handleControlInboxGet`.
  //
  // FAIL CLOSED, twice over:
  //   1. A present-but-malformed `?sender=` is a 400. It is never
  //      dropped in favour of an unfiltered page, because that would
  //      silently reinstate the starvation this parameter fixes while
  //      the client believed it had asked for a filtered drain.
  //   2. The value is a signed component of the canonical bytes, so
  //      adding, altering or removing `?sender=` in transit makes the
  //      signature stop verifying (401). An attacker cannot strip it.
  const rawSender = url.searchParams.get("sender");
  if (rawSender !== null && !isProtocolId(rawSender)) {
    return badRequest("sender must be a bounded identifier when present");
  }
  const senderFilter: string | null = rawSender;

  const user = await getUserForVerify(env.DB, userId);
  if (!user) return notFound();

  const message = canonicalControlInboxGetBytes({
    user_id: userId,
    timestamp_ms: ts,
    sender_id: senderFilter,
  });
  const pubBytes = safeDecodeBase64(user.ik_ed25519_pub);
  const sigBytes = safeDecodeBase64(sigB64);
  if (!pubBytes) {
    return unauthorized("no usable ed25519 signing key on file for this user");
  }
  if (!sigBytes) return badRequest("signature is not valid base64");
  const ok = await verifyEd25519(pubBytes, message, sigBytes);
  if (!ok) return unauthorized("signature verification failed");

  // Drain in FIFO order. The recipient's poll loop calls DELETE
  // per-row after apply; we don't auto-delete on read so a crash
  // between GET response and apply doesn't lose the SKDM.
  //
  // `recipient_id = ?` is bound from the *authenticated* userId in both
  // forms, so the filter narrows a page the caller was already entitled
  // to see. It cannot be used to enumerate or probe anybody else's rows.
  const now = Math.floor(Date.now() / 1000);
  const rows = senderFilter === null
    ? await env.DB.prepare(
        "SELECT id, sender_id, scope_id, bundle, created_at, kind FROM control_inbox " +
          "WHERE recipient_id = ? AND delivery_status = 'live' AND expires_at >= ? " +
          "ORDER BY created_at ASC LIMIT ?",
      )
        .bind(userId, now, MAX_DRAIN_ROWS)
        .all<{
          id: unknown;
          sender_id: string;
          scope_id: string;
          bundle: unknown;
          created_at: number;
          kind: string | null;
        }>()
    : await env.DB.prepare(
        "SELECT id, sender_id, scope_id, bundle, created_at, kind FROM control_inbox " +
          "WHERE recipient_id = ? AND sender_id = ? " +
          "AND delivery_status = 'live' AND expires_at >= ? " +
          "ORDER BY created_at ASC LIMIT ?",
      )
        .bind(userId, senderFilter, now, MAX_DRAIN_ROWS)
        .all<{
          id: unknown;
          sender_id: string;
          scope_id: string;
          bundle: unknown;
          created_at: number;
          kind: string | null;
        }>();

  const items = (rows.results || []).map((r) => {
    const idBytes = bytesToU8(r.id) ?? new Uint8Array(0);
    const bundleBytes = bytesToU8(r.bundle) ?? new Uint8Array(0);
    let bundleB64 = "";
    try {
      let bin = "";
      for (const b of bundleBytes) bin += String.fromCharCode(b);
      bundleB64 = btoa(bin);
    } catch {
      bundleB64 = "";
    }
    return {
      id: idToHex(idBytes),
      sender_id: r.sender_id,
      scope_id: r.scope_id,
      bundle_b64: bundleB64,
      created_at: r.created_at,
      // Additive. Lets a drain route a revocation without opening it, and lets a
      // client tell a burn notice apart from ordinary traffic before spending a
      // decrypt. Never authoritative: the lane label is the server's routing
      // hint, and the type byte inside the authenticated envelope is what
      // decides how a row is handled.
      kind: r.kind ?? "",
    };
  });

  // Echo the filter the signature authorised. A client that asked for a
  // filtered drain can assert this came back, so "the worker ignored my
  // filter" is detectable rather than silently served as an unfiltered
  // page. (An *old* worker cannot reach here at all with a filtered
  // request: it reconstructs the canonical bytes without the sender
  // component, so verification fails with a 401. The echo covers the
  // remaining case of a future worker that stops honouring it.)
  if (senderFilter !== null) {
    const state = await env.DB.prepare(
      `SELECT
          SUM(CASE WHEN delivery_status = 'live' THEN 1 ELSE 0 END)
            AS live_rows,
          SUM(CASE WHEN delivery_status = 'retryable' THEN 1 ELSE 0 END)
            AS retryable_rows,
          SUM(CASE WHEN delivery_status = 'quarantined' THEN 1 ELSE 0 END)
            AS quarantined_rows,
          SUM(CASE WHEN delivery_status = 'retired' THEN 1 ELSE 0 END)
            AS retired_rows
         FROM control_inbox
        WHERE recipient_id = ?
          AND sender_id = ?
          AND (
            delivery_status <> 'live'
            OR expires_at >= ?
          )
       `,
    ).bind(userId, senderFilter, now).first<{
      live_rows: number;
      retryable_rows: number;
      quarantined_rows: number;
      retired_rows: number;
    }>();
    const deliveryCounts = {
      live: state?.live_rows ?? 0,
      retryable: state?.retryable_rows ?? 0,
      quarantined: state?.quarantined_rows ?? 0,
      retired: state?.retired_rows ?? 0,
    };
    if (
      Object.values(deliveryCounts).some(
        (count) => !Number.isSafeInteger(count) || count < 0,
      )
    ) {
      throw new Error("control inbox delivery metadata is invalid");
    }
    return json({
      items,
      filtered_sender_id: senderFilter,
      filtered_sender_delivery: deliveryCounts,
    });
  }
  return json({ items });
}

export async function handleControlInboxDelete(
  request: Request,
  env: Env,
  inboxIdHex: string,
): Promise<Response> {
  if (!(await controlInboxDispositionSchemaReady(env.DB))) {
    return serviceUnavailable("control inbox schema unavailable");
  }
  // Drain deletes up to MAX_DRAIN_ROWS items per cycle; own bucket.
  const rl = await checkRateLimit(env, callerIp(request), 3600, "ci-del");
  if (!rl.ok) return tooMany(rl.retryAfter);

  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!isProtocolId(body.user_id)) return badRequest("user_id must be a bounded identifier");
  if (!isNonEmptyBase64(body.signature_b64))
    return badRequest("signature_b64 required");
  if (!freshnessOk(body.timestamp_ms))
    return badRequest("timestamp_ms required (within freshness window)");

  const id = hexToId(inboxIdHex);
  if (!id) return badRequest("inbox id must be 32 hex chars");

  const user = await getUserForVerify(env.DB, body.user_id);
  if (!user) return notFound();

  const message = canonicalControlInboxDeleteBytes({
    user_id: body.user_id,
    inbox_id_hex: inboxIdHex,
    timestamp_ms: body.timestamp_ms,
  });
  const pubBytes = safeDecodeBase64(user.ik_ed25519_pub);
  const sigBytes = safeDecodeBase64(body.signature_b64);
  if (!pubBytes) {
    return unauthorized("no usable ed25519 signing key on file for this user");
  }
  if (!sigBytes) return badRequest("signature is not valid base64");
  const ok = await verifyEd25519(pubBytes, message, sigBytes);
  if (!ok) return unauthorized("signature verification failed");

  // Scope the delete to rows owned by this recipient -- a leaked
  // inbox_id alone shouldn't let someone else nuke the row.
  await env.DB.prepare(
    "DELETE FROM control_inbox WHERE id = ? AND recipient_id = ?",
  )
    .bind(id, body.user_id)
    .run();

  return new Response(null, { status: 204 });
}
