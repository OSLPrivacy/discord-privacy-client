/// Bearer-token gate for operator-only administration. Public client
/// mutations use registered-identity signatures instead of a shared secret
/// that an open-source desktop binary could never keep confidential.

import type { Env } from "../env.js";
import { constantTimeTokenEqual } from "./crypto.js";
import { serviceUnavailable, unauthorized } from "./http.js";

/**
 * Run the admin-token check against an inbound request.
 *
 * Returns:
 *   - a 503 `Response` when the deployment omitted the token
 *   - a 401 `Response` when the header is missing or wrong
 *   - `null` only when the check passes
 *
 * Always performs the hash + compare even on an empty `provided` so
 * a totally-missing Authorization header doesn't take a different
 * timing path from a present-but-wrong one.
 */
export async function checkAdminToken(
  request: Request,
  env: Env,
): Promise<Response | null> {
  return checkToken(request, env.OSL_KEYSERVER_ADMIN_TOKEN, "admin");
}

/**
 * Owner comp issuance is intentionally protected by two independent bearer
 * secrets. Neither is shipped in a client, and both must be configured.
 */
export async function checkCompAdminTokens(
  request: Request,
  env: Env,
): Promise<Response | null> {
  const primary = await checkAdminToken(request, env);
  if (primary) return primary;
  if (!env.OSL_COMP_ADMIN_TOKEN) {
    console.error("[auth] comp token is not configured; failing closed");
    return serviceUnavailable("comp authorization not configured");
  }
  const header = request.headers.get("x-osl-comp-authorization") ?? "";
  const match = header.match(/^Bearer\s+(.+)$/i);
  const provided = (match?.[1] ?? "").trim();
  if (!provided || !(await constantTimeTokenEqual(provided, env.OSL_COMP_ADMIN_TOKEN))) {
    console.warn(`[auth] comp token check failed: method=${request.method}`);
    return unauthorized();
  }
  return null;
}

async function checkToken(
  request: Request,
  token: string | undefined,
  kind: "admin",
): Promise<Response | null> {
  if (!token || token.length === 0) {
    console.error(`[auth] ${kind} token is not configured; failing closed`);
    return serviceUnavailable(`${kind} authorization not configured`);
  }
  const header = request.headers.get("authorization") ?? "";
  let provided = "";
  const m = header.match(/^Bearer\s+(.+)$/i);
  if (m) provided = (m[1] ?? "").trim();
  const ok = provided.length > 0 && (await constantTimeTokenEqual(provided, token));
  if (!ok) {
    // Never log the URL: signed protocol requests currently put opaque IDs
    // and signatures in path/query components.
    console.warn(`[auth] ${kind} token check failed: method=${request.method}`);
    return unauthorized();
  }
  return null;
}

// REGISTER-FIX: `isUserAllowed` / `OSL_KEYSERVER_ALLOWED_USERS` were
// retired with the move to open signed registration. `/v1/register`
// was the allowlist's only consumer; key-control + first-write-wins
// (see endpoints/register.ts) replace it. No allowlist gate remains
// anywhere in the keyserver.
