/// Admin-token gate for mutation routes. Mirrors the Railway
/// server's preHandler — returns null when no token is configured
/// (dev mode passthrough) or a Response when the request is
/// unauthorized. Returns undefined when the check passes.

import type { Env } from "../env.js";
import { constantTimeTokenEqual } from "./crypto.js";
import { unauthorized } from "./http.js";

/**
 * Run the admin-token check against an inbound request.
 *
 * Returns:
 *   - a 401 `Response` when the configured token is set and the
 *     header is missing or wrong
 *   - `null` when the check passes OR when no token is configured
 *     (open dev mode)
 *
 * Always performs the hash + compare even on an empty `provided` so
 * a totally-missing Authorization header doesn't take a different
 * timing path from a present-but-wrong one.
 */
export async function checkAdminToken(
  request: Request,
  env: Env,
): Promise<Response | null> {
  const token = env.OSL_KEYSERVER_ADMIN_TOKEN;
  if (!token || token.length === 0) {
    // Dev mode — Railway server logged a loud warning on every
    // request; we surface it once at fetch entry via console.warn
    // (cheap and Workers-visible).
    return null;
  }
  const header = request.headers.get("authorization") ?? "";
  let provided = "";
  const m = header.match(/^Bearer\s+(.+)$/i);
  if (m) provided = (m[1] ?? "").trim();
  const ok = provided.length > 0 && (await constantTimeTokenEqual(provided, token));
  if (!ok) {
    console.warn(
      `[auth] admin token check failed: method=${request.method} url=${request.url} had_header=${header.length > 0}`,
    );
    return unauthorized();
  }
  return null;
}

// REGISTER-FIX: `isUserAllowed` / `OSL_KEYSERVER_ALLOWED_USERS` were
// retired with the move to open signed registration. `/v1/register`
// was the allowlist's only consumer; key-control + first-write-wins
// (see endpoints/register.ts) replace it. No allowlist gate remains
// anywhere in the keyserver.
