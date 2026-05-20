/// Minimal HTTP helpers. Mirrors keyserver-cf/src/lib/http.ts in
/// spirit but kept tiny -- the cipher-store only emits JSON error
/// bodies and raw-bytes success bodies, no CORS surface yet (the
/// Tauri webview makes same-origin-ish requests through the OSL
/// invoke bridge, not browser fetch).

export function json(
  body: unknown,
  status: number = 200,
  extraHeaders: Record<string, string> = {}
): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": "no-store",
      ...extraHeaders,
    },
  });
}

export function error(
  status: number,
  code: string,
  message: string
): Response {
  return json({ error: code, message }, status);
}

export function notFound(): Response {
  return error(404, "not_found", "no such route or blob");
}

export function serverError(): Response {
  // Intentionally generic. Stack traces stay server-side (and even
  // there we don't persist them -- console.error goes to wrangler
  // tail only, which operators must not pipe to disk).
  return error(500, "internal_error", "server failure");
}

/// Read a client IP for rate-limit bucketing. Cloudflare's
/// `CF-Connecting-IP` is the canonical source; falls back to
/// `x-forwarded-for` first hop in unusual deployments. Note: this
/// value is ONLY used as a transient rate-limit key in KV with a
/// short TTL; it is never written to D1 or any log.
export function clientIp(request: Request): string {
  const cf = request.headers.get("cf-connecting-ip");
  if (cf) return cf;
  const xff = request.headers.get("x-forwarded-for");
  if (xff) {
    const first = xff.split(",")[0];
    if (first) return first.trim();
  }
  return "unknown";
}
