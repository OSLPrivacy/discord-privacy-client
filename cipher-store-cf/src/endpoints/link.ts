/// View-once link lane: the path for a recipient who has no OSL and no
/// account.
///
/// ## Routes
///
///   POST /v1/link                  create (OSL clients only, grant-gated)
///   POST /v1/link/:id/status       sender-side status (manage token)
///   DELETE /v1/link/:id            sender-side revoke (manage token)
///   GET  /v/:id                    landing page -- see lib/landing.ts
///   POST /v/:id/fetch              release ciphertext (gesture-gated)
///   POST /v/:id/burn               early destroy (best effort)
///
/// ## Why the burn is not on fetch
///
/// Discord fetches every URL it sees to build a preview. A naive
/// one-time link is therefore consumed by a bot before the human ever
/// clicks. Three things stop that here:
///
///   1. `GET /v/:id` is a static, content-free page, byte-identical for
///      every id, always 200. Fetching it reveals nothing and consumes
///      nothing.
///   2. Ciphertext moves only on `POST /v/:id/fetch`, which requires the
///      capability token **in the request body** (never a URL, so it
///      cannot land in an access log), a same-origin `Origin`, and a
///      header the page sets only after an `isTrusted` gesture. A
///      crawler that executes JS still cannot click.
///   3. The first fetch does not delete. It reserves for 60 seconds and
///      returns the bytes; repeat fetches inside that window return the
///      same bytes, so a flaky network cannot destroy content the
///      recipient never saw. At `reserved_until` the ciphertext is
///      destroyed regardless of client confirmation.
///
/// The enforceable claim is exactly: **the link dies after one view, or
/// 60 seconds, whichever is first.**
///
/// ## What is never stored
///
/// No IP, no user agent, no referrer, no timing detail, no identity.
/// `retrieved_at` is unix seconds and `retrieval_count` is a small
/// integer; that is the entire retrieval record.

import type { Env } from "../env.js";
import { error, json } from "../lib/http.js";
import { constantTimeEqualHex, sha256Hex } from "../lib/digest.js";
import { verifyLinkGrant } from "../lib/link-grant.js";
import { readBoundedBody } from "./blob.js";

/// D1 keeps this comfortably inside its per-value ceiling and keeps a
/// one-shot link small. Larger images must be downscaled client-side.
export const MAX_LINK_BYTES = 256 * 1024;

/// The sender-facing warning says "after 1 hour". This is the only TTL
/// accepted, so that sentence is literally true.
export const LINK_TTL_SECONDS = 3600;

/// The reservation window. One view, or this, whichever is first.
export const RESERVATION_SECONDS = 60;

/// How long a content-free receipt row outlives expiry, so the sender
/// can still be shown "Retrieved at HH:MM" or "Expired without being
/// retrieved". The row holds no ciphertext and no identifiers.
export const RECEIPT_RETENTION_SECONDS = 24 * 60 * 60;

const TOKEN_RE = /^[0-9a-f]{32}$/;
const ID_RE = /^[0-9a-f]{32}$/;

interface LinkRow {
  data: unknown;
  size_bytes: number;
  created_at: number;
  expires_at: number;
  fetch_token_sha256_hex: string;
  manage_token_sha256_hex: string;
  retrieved_at: number | null;
  retrieval_count: number;
  reserved_until: number | null;
}

/// One shape for every negative outcome on the recipient path: no such
/// link, wrong token, expired, already burned. A caller cannot tell
/// them apart, so nothing here is an oracle either.
function unavailable(): Response {
  return json(
    { error: "unavailable", message: "this link is no longer available" },
    404,
    noStoreHeaders(),
  );
}

function noStoreHeaders(): Record<string, string> {
  return {
    "cache-control": "no-store",
    "referrer-policy": "no-referrer",
    "x-robots-tag": "noindex, nofollow, noarchive, nosnippet",
    "x-content-type-options": "nosniff",
  };
}

function newLinkId(): string {
  // 16 random bytes. Per-link variety comes from this, not from a
  // rotating domain fleet -- see README "Domain and abuse posture".
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  let hex = "";
  for (const b of bytes) hex += b.toString(16).padStart(2, "0");
  return hex;
}

/// Same-origin proof for the recipient routes. `fetch()` always sets
/// `Origin` on a POST, so a missing header is a non-browser caller.
function originOk(request: Request): boolean {
  const url = new URL(request.url);
  const origin = request.headers.get("origin");
  if (origin !== url.origin) return false;
  const site = request.headers.get("sec-fetch-site");
  if (site !== null && site !== "same-origin") return false;
  return true;
}

async function readTokenBody(request: Request): Promise<string | null> {
  const contentType = request.headers.get("content-type") ?? "";
  if (!contentType.toLowerCase().startsWith("application/json")) return null;
  const body = await readBoundedBody(request, 1024);
  if (body.status !== "ok") return null;
  let parsed: { t?: unknown };
  try {
    parsed = JSON.parse(new TextDecoder().decode(body.bytes));
  } catch {
    return null;
  }
  const t = parsed.t;
  if (typeof t !== "string") return null;
  const lower = t.trim().toLowerCase();
  return TOKEN_RE.test(lower) ? lower : null;
}

function readHexHeader(request: Request, name: string): string | null {
  const raw = request.headers.get(name);
  if (raw === null) return null;
  const lower = raw.trim().toLowerCase();
  return TOKEN_RE.test(lower) ? lower : null;
}

function blobToBytes(v: unknown): Uint8Array {
  if (v == null) return new Uint8Array(0);
  if (v instanceof Uint8Array) return v;
  if (v instanceof ArrayBuffer) return new Uint8Array(v);
  if (ArrayBuffer.isView(v)) {
    const view = v as ArrayBufferView;
    return new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  }
  if (Array.isArray(v)) return new Uint8Array(v as number[]);
  if (typeof v === "string") {
    try {
      const bin = atob(v);
      const arr = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
      return arr;
    } catch {
      return new TextEncoder().encode(v);
    }
  }
  return new Uint8Array(0);
}

/// Destroy the ciphertext, keep the content-free receipt.
async function destroyCiphertext(env: Env, id: string, now: number): Promise<void> {
  await env.DB.prepare(
    `UPDATE view_once_links
       SET data = NULL, size_bytes = 0, reserved_until = NULL, burned_at = ?
     WHERE id = ? AND data IS NOT NULL`,
  )
    .bind(now, id)
    .run();
}

export async function handleLinkCreate(request: Request, env: Env): Promise<Response> {
  const grant = await verifyLinkGrant(request, env);
  if (!grant.ok) return error(grant.status, grant.code, grant.message);

  const ttlHeader = request.headers.get("x-osl-ttl-seconds");
  if (ttlHeader !== String(LINK_TTL_SECONDS)) {
    return error(
      400,
      "bad_ttl",
      `X-OSL-TTL-Seconds must be ${LINK_TTL_SECONDS} for a view-once link`,
    );
  }

  const fetchToken = readHexHeader(request, "x-osl-fetch-token");
  const manageToken = readHexHeader(request, "x-osl-manage-token");
  if (fetchToken === null || manageToken === null) {
    return error(
      400,
      "bad_token",
      "X-OSL-Fetch-Token and X-OSL-Manage-Token must each be 32 hex chars",
    );
  }
  if (constantTimeEqualHex(fetchToken, manageToken)) {
    return error(
      400,
      "bad_token",
      "the fetch and manage capabilities must be distinct",
    );
  }

  const declared = request.headers.get("content-length");
  if (declared !== null) {
    if (!/^\d+$/.test(declared)) {
      return error(400, "bad_content_length", "Content-Length must be an unsigned integer");
    }
    if (Number(declared) > MAX_LINK_BYTES) {
      return error(413, "too_large", `link payload exceeds ${MAX_LINK_BYTES} bytes`);
    }
  }

  const body = await readBoundedBody(request, MAX_LINK_BYTES);
  if (body.status === "too_large") {
    return error(413, "too_large", `link payload exceeds ${MAX_LINK_BYTES} bytes`);
  }
  const data = body.bytes;
  // 12-byte nonce + at least a 16-byte tag.
  if (data.length <= 28) {
    return error(400, "empty_body", "link payload required");
  }

  const now = Math.floor(Date.now() / 1000);
  const expiresAt = now + LINK_TTL_SECONDS;
  const fetchDigest = await sha256Hex(fetchToken);
  const manageDigest = await sha256Hex(manageToken);

  for (let attempt = 0; attempt < 5; attempt++) {
    const id = newLinkId();
    try {
      await env.DB.prepare(
        `INSERT INTO view_once_links
           (id, data, size_bytes, created_at, expires_at,
            fetch_token_sha256_hex, manage_token_sha256_hex, retrieval_count)
         VALUES (?, ?, ?, ?, ?, ?, ?, 0)`,
      )
        .bind(id, data, data.length, now, expiresAt, fetchDigest, manageDigest)
        .run();
      // The URL is assembled client-side: the key and the fetch token go
      // in the fragment, which never reaches this server.
      return json({ id, expires_at: expiresAt }, 201, noStoreHeaders());
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes("UNIQUE") || msg.includes("PRIMARY")) continue;
      throw err;
    }
  }
  return error(500, "id_collision_loop", "could not allocate a fresh ID after retries");
}

export async function handleLinkFetch(
  request: Request,
  env: Env,
  id: string,
): Promise<Response> {
  if (!ID_RE.test(id)) return unavailable();
  if (!originOk(request)) return unavailable();
  // Set by the page only after an isTrusted pointerdown/keydown on the
  // reveal button. A crawler executing the page cannot produce it.
  if (request.headers.get("x-osl-gesture") !== "1") return unavailable();

  const token = await readTokenBody(request);
  if (token === null) return unavailable();

  const row = await env.DB.prepare(
    `SELECT data, size_bytes, created_at, expires_at, fetch_token_sha256_hex,
            manage_token_sha256_hex, retrieved_at, retrieval_count, reserved_until
       FROM view_once_links WHERE id = ? LIMIT 1`,
  )
    .bind(id)
    .first<LinkRow>();
  if (!row) return unavailable();

  const now = Math.floor(Date.now() / 1000);
  const presentedDigest = await sha256Hex(token);
  if (!constantTimeEqualHex(row.fetch_token_sha256_hex, presentedDigest)) {
    return unavailable();
  }
  if (row.data === null || row.data === undefined) return unavailable();
  if (row.expires_at < now) {
    // Expired between sweeps. Destroy on sight rather than waiting.
    await destroyCiphertext(env, id, now);
    return unavailable();
  }
  if (row.reserved_until !== null && row.reserved_until < now) {
    // The reservation closed. The sweep would get here within minutes;
    // do not let that lag become a hole.
    await destroyCiphertext(env, id, now);
    return unavailable();
  }

  if (row.reserved_until === null) {
    const claimed = await env.DB.prepare(
      `UPDATE view_once_links
         SET reserved_until = ?, retrieved_at = ?, retrieval_count = 1
       WHERE id = ? AND reserved_until IS NULL AND data IS NOT NULL`,
    )
      .bind(now + RESERVATION_SECONDS, now, id)
      .run();
    // A lost race means another request reserved first; that request is
    // inside the same window, so serving the same bytes is correct.
    void claimed;
  } else {
    await env.DB.prepare(
      `UPDATE view_once_links
         SET retrieval_count = retrieval_count + 1
       WHERE id = ? AND reserved_until IS NOT NULL AND data IS NOT NULL`,
    )
      .bind(id)
      .run();
  }

  const bytes = blobToBytes(row.data);
  return new Response(bytes, {
    status: 200,
    headers: {
      "content-type": "application/octet-stream",
      "content-length": String(bytes.byteLength),
      ...noStoreHeaders(),
    },
  });
}

export async function handleLinkBurn(
  request: Request,
  env: Env,
  id: string,
): Promise<Response> {
  // Deliberately does NOT require the gesture header: burning early is
  // always safe, and the page fires it from pagehide/blur handlers.
  if (!ID_RE.test(id)) return new Response(null, { status: 204, headers: noStoreHeaders() });
  if (!originOk(request)) {
    return new Response(null, { status: 204, headers: noStoreHeaders() });
  }
  const token = await readTokenBody(request);
  if (token !== null) {
    const row = await env.DB.prepare(
      "SELECT fetch_token_sha256_hex FROM view_once_links WHERE id = ? LIMIT 1",
    )
      .bind(id)
      .first<{ fetch_token_sha256_hex: string }>();
    if (row) {
      const digest = await sha256Hex(token);
      if (constantTimeEqualHex(row.fetch_token_sha256_hex, digest)) {
        await destroyCiphertext(env, id, Math.floor(Date.now() / 1000));
      }
    }
  }
  // Always 204, always the same shape: no oracle on this route either.
  return new Response(null, { status: 204, headers: noStoreHeaders() });
}

/// Sender-facing status. Vocabulary is deliberately constrained to
/// "created" / "retrieved" / "expired": retrieval is not viewing, and
/// this lane must never claim a message was read, seen or viewed.
export type LinkState = "created" | "retrieved" | "expired";

export async function handleLinkStatus(
  request: Request,
  env: Env,
  id: string,
): Promise<Response> {
  if (!ID_RE.test(id)) return unavailable();
  const token = await readTokenBody(request);
  if (token === null) return unavailable();
  const row = await env.DB.prepare(
    `SELECT data, expires_at, manage_token_sha256_hex, retrieved_at, retrieval_count
       FROM view_once_links WHERE id = ? LIMIT 1`,
  )
    .bind(id)
    .first<Pick<
      LinkRow,
      "data" | "expires_at" | "manage_token_sha256_hex" | "retrieved_at" | "retrieval_count"
    >>();
  if (!row) {
    // Purged. We cannot distinguish, and we will not guess in the
    // direction that sounds better.
    return json({ state: "expired", retrieved_at: null, retrieval_count: 0 }, 200, noStoreHeaders());
  }
  const digest = await sha256Hex(token);
  if (!constantTimeEqualHex(row.manage_token_sha256_hex, digest)) return unavailable();

  const now = Math.floor(Date.now() / 1000);
  let state: LinkState;
  if (row.retrieved_at !== null) state = "retrieved";
  else if (row.data === null || row.data === undefined || row.expires_at < now) state = "expired";
  else state = "created";

  return json(
    {
      state,
      retrieved_at: row.retrieved_at,
      retrieval_count: row.retrieval_count,
    },
    200,
    noStoreHeaders(),
  );
}

/// Sender-side revoke. Destroys the ciphertext immediately; the receipt
/// row survives so the sender still sees a truthful status.
export async function handleLinkRevoke(
  request: Request,
  env: Env,
  id: string,
): Promise<Response> {
  if (!ID_RE.test(id)) return unavailable();
  const token = await readTokenBody(request);
  if (token === null) return unavailable();
  const row = await env.DB.prepare(
    "SELECT manage_token_sha256_hex FROM view_once_links WHERE id = ? LIMIT 1",
  )
    .bind(id)
    .first<{ manage_token_sha256_hex: string }>();
  if (!row) return new Response(null, { status: 204, headers: noStoreHeaders() });
  const digest = await sha256Hex(token);
  if (!constantTimeEqualHex(row.manage_token_sha256_hex, digest)) return unavailable();
  await destroyCiphertext(env, id, Math.floor(Date.now() / 1000));
  return new Response(null, { status: 204, headers: noStoreHeaders() });
}
