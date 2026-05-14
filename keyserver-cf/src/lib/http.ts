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
