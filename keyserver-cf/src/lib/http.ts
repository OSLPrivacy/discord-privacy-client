/// Response helpers — keep the wire JSON shape identical to the
/// Fastify Railway server so the existing Rust client deserialises
/// without modification.

const JSON_HEADERS = { "content-type": "application/json; charset=utf-8" };

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

export function serverError(message: string): Response {
  return error(500, message);
}

export function serviceUnavailable(message: string): Response {
  return error(503, message);
}

/// CORS — applied ONLY to the two browser-callable endpoints
/// (/v1/checkout-session, /v1/billing-portal-session). The OSL
/// client and the Stripe webhook are never called from a browser
/// and deliberately get no CORS headers.
const CORS_ORIGIN = "https://oslprivacy.com";

export function corsPreflight(methods = "POST, OPTIONS"): Response {
  return new Response(null, {
    status: 204,
    headers: {
      "Access-Control-Allow-Origin": CORS_ORIGIN,
      "Access-Control-Allow-Methods": methods,
      "Access-Control-Allow-Headers": "Content-Type",
      "Access-Control-Max-Age": "86400",
    },
  });
}

export function withCors(res: Response): Response {
  const out = new Response(res.body, {
    status: res.status,
    statusText: res.statusText,
    headers: new Headers(res.headers),
  });
  out.headers.set("Access-Control-Allow-Origin", CORS_ORIGIN);
  return out;
}
