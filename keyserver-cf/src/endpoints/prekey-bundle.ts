import type { Env } from "../env.js";
import { checkAdminToken } from "../lib/auth.js";
import { canonicalReplenishBytes } from "../lib/canonical.js";
import { verifyEd25519 } from "../lib/crypto.js";
import {
  getUserForVerify,
  OpkIdConflict,
  popPrekeyBundle,
  upsertPrekeyBundle,
} from "../lib/db.js";
import {
  badRequest,
  conflict,
  json,
  notFound,
  tooMany,
  unauthorized,
} from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import {
  decodeBase64,
  isNonEmptyBase64,
  isNonNegativeInt,
  isPlainString,
} from "../lib/validation.js";

// ---- GET /v1/prekey-bundle/:user_id ----

export async function handlePrekeyBundleGet(
  env: Env,
  userId: string,
): Promise<Response> {
  const bundle = await popPrekeyBundle(env.DB, userId);
  if (!bundle) {
    return notFound("unknown user_id or no prekey bundle uploaded");
  }
  return json(bundle);
}

// ---- POST /v1/prekey-bundle/replenish ----

export async function handlePrekeyBundleReplenish(
  request: Request,
  env: Env,
): Promise<Response> {
  const authErr = await checkAdminToken(request, env);
  if (authErr) return authErr;
  const rl = await checkRateLimit(env, callerIp(request), 10);
  if (!rl.ok) return tooMany(rl.retryAfter);

  let b: Record<string, unknown>;
  try {
    b = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!isPlainString(b.user_id)) return badRequest("user_id required");
  if (!isNonEmptyBase64(b.batch_signature_b64)) {
    return badRequest("batch_signature_b64 required");
  }
  if (!Array.isArray(b.opks)) return badRequest("opks must be an array");

  const opks: { id: number; pub_b64: string }[] = [];
  for (const raw of b.opks) {
    const o = raw as { id?: unknown; pub_b64?: unknown };
    if (!isNonNegativeInt(o.id)) return badRequest("opk.id must be u32");
    if (!isNonEmptyBase64(o.pub_b64)) return badRequest("opk.pub_b64 must be base64");
    opks.push({ id: o.id, pub_b64: o.pub_b64 });
  }
  let spk: { pub_b64: string; signature_b64: string; rotated_at: string } | null = null;
  if (b.spk != null) {
    const s = b.spk as {
      pub_b64?: unknown;
      signature_b64?: unknown;
      rotated_at?: unknown;
    };
    if (
      !isNonEmptyBase64(s.pub_b64) ||
      !isNonEmptyBase64(s.signature_b64) ||
      typeof s.rotated_at !== "string" ||
      Number.isNaN(Date.parse(s.rotated_at))
    ) {
      return badRequest("spk fields malformed");
    }
    spk = { pub_b64: s.pub_b64, signature_b64: s.signature_b64, rotated_at: s.rotated_at };
  }

  const user = await getUserForVerify(env.DB, b.user_id);
  if (!user) return notFound("unknown user_id — register before replenish");

  const message = canonicalReplenishBytes({
    user_id: b.user_id,
    spk,
    opks,
  });
  const ikEd25519 = decodeBase64(user.ik_ed25519_pub);
  const sig = decodeBase64(b.batch_signature_b64);
  const ok = await verifyEd25519(ikEd25519, message, sig);
  if (!ok) return unauthorized("batch_signature_b64 verification failed");

  try {
    await upsertPrekeyBundle(env.DB, b.user_id, spk, opks);
  } catch (err) {
    if (err instanceof OpkIdConflict) return conflict("opk id already used");
    throw err;
  }
  return json({ user_id: b.user_id, opks_added: opks.length });
}
