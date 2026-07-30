/// Response helpers — keep the wire JSON shape identical to the
/// Fastify Railway server so the existing Rust client deserialises
/// without modification.

const JSON_HEADERS = {
  "content-type": "application/json; charset=utf-8",
  "cache-control": "no-store",
  "x-content-type-options": "nosniff",
  "referrer-policy": "no-referrer",
};

export function json(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    ...init,
    headers: { ...JSON_HEADERS, ...(init?.headers ?? {}) },
  });
}

export function error(status: number, message: string): Response {
  return json({ error: message }, { status });
}

export function notFound(message = "not found"): Response {
  return error(404, message);
}

export function badRequest(message: string): Response {
  return error(400, message);
}

export function unauthorized(message = "unauthorized"): Response {
  return error(401, message);
}

export function forbidden(message: string): Response {
  return error(403, message);
}

export function conflict(message: string): Response {
  return error(409, message);
}

export function gone(message: string): Response {
  return error(410, message);
}

export function tooMany(retryAfterSec: number): Response {
  return new Response(JSON.stringify({ error: "rate_limited" }), {
    status: 429,
    headers: {
      ...JSON_HEADERS,
      "retry-after": String(retryAfterSec),
    },
  });
}

/// Which pending-row cap was hit. Both mean the same thing to the sender
/// (only the recipient draining clears it) but they are diagnostically
/// different: "recipient" is the global per-recipient cap -- that inbox is
/// swamped, possibly by many senders -- while "sender_recipient" is the
/// per-pair cap, which is the one a normal conversation reaches first.
export type InboxFullScope = "recipient" | "sender_recipient";

/// 429, but NOT a rate limit. Pending-row quota exhaustion is durable: the
/// rows only disappear when the recipient drains their inbox, or after the
/// 7-day TTL. Telling the sender "rate_limited" says "slow down", which is
/// actively false -- waiting will never help.
///
/// Status stays 429 (not 507/409) so already-deployed clients that branch
/// on status alone keep working; the distinction is carried in the body.
/// The kind is a secondary `scope` field rather than a second top-level
/// error code so a client has exactly one string to match for "recipient
/// cannot receive this" -- see the `recipient_inbox_full` match in
/// crates/keystore/src/client.rs.
export function recipientInboxFull(
  retryAfterSec: number,
  scope: InboxFullScope,
): Response {
  return new Response(JSON.stringify({ error: "recipient_inbox_full", scope }), {
    status: 429,
    headers: {
      ...JSON_HEADERS,
      "retry-after": String(retryAfterSec),
    },
  });
}

/// 507 Insufficient Storage — the bilateral-burn revocation lane for this
/// recipient (or this sender/recipient pair) is full.
///
/// Deliberately NOT the 429 `recipientInboxFull` shape, and deliberately not an
/// eviction:
///
/// - Revocation rows are never evicted. `evictOldestPending` silently deletes
///   the oldest undelivered rows, which for a burn means the sender's own next
///   32 messages destroy a burn queued to an offline peer, with no notice to
///   anyone. Refusing is the only outcome that keeps the sender informed.
/// - 507 rather than 429 because there is no deployed client to keep compatible:
///   the lane is new, so its refusal can have an honest, unambiguous status
///   instead of borrowing "rate limited" and being read as "slow down".
///
/// The sender retries from its durable revocation outbox, so a full lane delays
/// a burn notice; it never loses one.
export function revocationLaneFull(
  retryAfterSec: number,
  scope: InboxFullScope,
): Response {
  return new Response(
    JSON.stringify({ error: "revocation_lane_full", scope }),
    {
      status: 507,
      headers: {
        ...JSON_HEADERS,
        "retry-after": String(retryAfterSec),
      },
    },
  );
}

export function serverError(message: string): Response {
  return error(500, message);
}

export function serviceUnavailable(message: string): Response {
  return error(503, message);
}

/// CORS — applied only to explicitly browser-callable commerce and update
/// endpoints. Localhost origins are exact development ports, never wildcards.
const CORS_ORIGINS = new Set([
  "https://oslprivacy.com",
  "https://www.oslprivacy.com",
  "http://127.0.0.1:4173",
  "http://localhost:4173",
]);

function corsOrigin(request?: Request): string {
  const requested = request?.headers.get("origin");
  return requested && CORS_ORIGINS.has(requested) ? requested : "https://oslprivacy.com";
}

export function corsPreflight(
  methods = "POST, OPTIONS",
  request?: Request,
): Response {
  return new Response(null, {
    status: 204,
    headers: {
      "Access-Control-Allow-Origin": corsOrigin(request),
      "Access-Control-Allow-Methods": methods,
      "Access-Control-Allow-Headers": "Content-Type",
      "Access-Control-Max-Age": "86400",
    },
  });
}

export function withCors(res: Response, request?: Request): Response {
  const out = new Response(res.body, {
    status: res.status,
    statusText: res.statusText,
    headers: new Headers(res.headers),
  });
  out.headers.set("Access-Control-Allow-Origin", corsOrigin(request));
  out.headers.set("Vary", "Origin");
  return out;
}
