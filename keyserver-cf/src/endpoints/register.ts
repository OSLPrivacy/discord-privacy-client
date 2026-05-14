import type { Env } from "../env.js";
import { checkAdminToken, isUserAllowed } from "../lib/auth.js";
import { upsertUser } from "../lib/db.js";
import { badRequest, forbidden, json } from "../lib/http.js";
import { isNonEmptyBase64, isPlainString } from "../lib/validation.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { tooMany } from "../lib/http.js";

export async function handleRegister(request: Request, env: Env): Promise<Response> {
  const authErr = await checkAdminToken(request, env);
  if (authErr) return authErr;
  const rl = await checkRateLimit(env, callerIp(request), 10);
  if (!rl.ok) return tooMany(rl.retryAfter);

  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }

  const required = [
    "user_id",
    "ik_x25519_pub",
    "ik_ed25519_pub",
    "ik_mlkem768_pub",
    "ik_x25519_signature",
  ] as const;
  for (const field of required) {
    if (!(field in body)) return badRequest(`missing field: ${field}`);
  }
  if (!isPlainString(body.user_id)) {
    return badRequest("user_id must be a non-empty string");
  }
  // Allowlist gate runs post-auth so an allowlist miss logs the
  // *valid-token* attempt — useful operator signal.
  if (!isUserAllowed(env, body.user_id)) {
    console.warn(
      `[register] allowlist check failed (token was valid): user_id=${body.user_id}`,
    );
    return forbidden("forbidden: user_id not on allowlist");
  }
  for (const k of [
    "ik_x25519_pub",
    "ik_ed25519_pub",
    "ik_mlkem768_pub",
    "ik_x25519_signature",
  ] as const) {
    if (!isNonEmptyBase64(body[k])) {
      return badRequest(`${k} must be base64`);
    }
  }
  // ik_ratchet_initial_pub optional; null or absent → null in DB.
  if (
    body.ik_ratchet_initial_pub !== undefined &&
    body.ik_ratchet_initial_pub !== null &&
    !isNonEmptyBase64(body.ik_ratchet_initial_pub)
  ) {
    return badRequest("ik_ratchet_initial_pub must be base64 (when present)");
  }

  const result = await upsertUser(env.DB, {
    user_id: body.user_id,
    ik_x25519_pub: body.ik_x25519_pub as string,
    ik_ed25519_pub: body.ik_ed25519_pub as string,
    ik_mlkem768_pub: body.ik_mlkem768_pub as string,
    ik_x25519_signature: body.ik_x25519_signature as string,
    ik_ratchet_initial_pub: (body.ik_ratchet_initial_pub as string | null | undefined) ?? null,
  });

  if (result.isNew) {
    return json(
      { user_id: body.user_id, registered_at: result.registered_at },
      { status: 201 },
    );
  }
  return json({
    user_id: body.user_id,
    key_rotation_recorded: true,
    last_rotated_at: result.last_rotated_at,
  });
}
