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
