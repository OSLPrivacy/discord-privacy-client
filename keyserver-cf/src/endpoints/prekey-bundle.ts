import type { Env } from "../env.js";
import {
  canonicalPrekeyBundleGetBytes,
  canonicalReplenishBytes,
  CONSUMING_GET_FRESHNESS_WINDOW_MS,
  SIGNED_COMMAND_FRESHNESS_WINDOW_MS,
} from "../lib/canonical.js";
import { verifyEd25519 } from "../lib/crypto.js";
import {
  getUserForVerify,
  OpkIdConflict,
  OpkPoolQuotaExceeded,
  ConsumingGetReplay,
  popPrekeyBundleAuthenticated,
  upsertPrekeyBundleAuthenticated,
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
  isHighEntropyRequestId,
  isNonEmptyBase64,
  isProtocolId,
  isU32,
} from "../lib/validation.js";

export const MAX_OPK_REPLENISH_BATCH = 100;
const X25519_PUBLIC_KEY_BYTES = 32;
const ED25519_SIGNATURE_BYTES = 64;

// ---- GET /v1/prekey-bundle/:user_id ----

export async function handlePrekeyBundleGet(
  request: Request,
  env: Env,
  recipientId: string,
): Promise<Response> {
  const url = new URL(request.url);
  const requesterId = url.searchParams.get("requester_id") ?? "";
  const signedRecipientId = url.searchParams.get("recipient_id") ?? "";
  const timestampMs = Number(url.searchParams.get("ts"));
  const signatureB64 = url.searchParams.get("sig") ?? "";
  if (
    !isProtocolId(requesterId) ||
    !isProtocolId(signedRecipientId) ||
    !isProtocolId(recipientId) ||
    !Number.isSafeInteger(timestampMs) ||
    timestampMs <= 0 ||
    Math.abs(Date.now() - timestampMs) > CONSUMING_GET_FRESHNESS_WINDOW_MS ||
    !isNonEmptyBase64(signatureB64)
  ) {
    return unauthorized("fresh signed requester authorization required");
  }
  if (signedRecipientId !== recipientId) {
    return unauthorized("signed recipient does not match request target");
  }

  const requester = await getUserForVerify(env.DB, requesterId);
  if (!requester) return unauthorized("requester identity is not registered");
  const message = canonicalPrekeyBundleGetBytes({
    requester_id: requesterId,
    recipient_id: recipientId,
    timestamp_ms: timestampMs,
  });
  let pubBytes: Uint8Array;
  let sigBytes: Uint8Array;
  try {
    pubBytes = decodeBase64(requester.ik_ed25519_pub);
    sigBytes = decodeBase64(signatureB64);
  } catch {
    return unauthorized("signature encoding invalid");
  }
  if (!(await verifyEd25519(pubBytes, message, sigBytes))) {
    return unauthorized("signature verification failed");
  }
  // A valid identity may still be malicious. Bound both one actor's
  // aggregate consumption and aggregate pressure on one recipient so
  // fresh timestamps or many throwaway identities cannot rapidly drain
  // the target's OPK pool.
  const [requesterLimit, recipientLimit] = await Promise.all([
    checkRateLimit(env, requesterId, 120, "prekey-get-requester"),
    // A normal client consumes one bundle when establishing a new session,
    // not once per message. Ten fresh sessions per recipient per minute keeps
    // ordinary multi-device/group use working while making anonymous pool
    // draining at least an order of magnitude slower.
    checkRateLimit(env, recipientId, 10, "prekey-get-recipient"),
  ]);
  if (!requesterLimit.ok || !recipientLimit.ok) {
    return tooMany(
      Math.max(requesterLimit.retryAfter, recipientLimit.retryAfter),
    );
  }
  const requestDigest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", message),
  );
  let result;
  try {
    result = await popPrekeyBundleAuthenticated(
      env.DB,
      recipientId,
      requesterId,
      requestDigest,
      requester.ik_ed25519_pub,
    );
  } catch (err) {
    if (err instanceof ConsumingGetReplay) {
      return conflict("signed prekey request already consumed");
    }
    throw err;
  }
  if (result.status === "stale_identity") {
    return unauthorized("requester identity changed during authorization");
  }
  if (result.status === "not_found") {
    return notFound("unknown user_id or no prekey bundle uploaded");
  }
  return json(result.bundle);
}

// ---- POST /v1/prekey-bundle/replenish ----

export async function handlePrekeyBundleReplenish(
  request: Request,
  env: Env,
): Promise<Response> {
  const rl = await checkRateLimit(env, callerIp(request), 10, "prekey-replenish");
  if (!rl.ok) return tooMany(rl.retryAfter);

  let b: Record<string, unknown>;
  try {
    b = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!isProtocolId(b.user_id)) return badRequest("user_id must be a bounded identifier");
  if (
    !Number.isSafeInteger(b.timestamp_ms) ||
    (b.timestamp_ms as number) <= 0 ||
    Math.abs(Date.now() - (b.timestamp_ms as number)) >
      SIGNED_COMMAND_FRESHNESS_WINDOW_MS
  ) {
    return unauthorized("fresh signed replenish request required");
  }
  if (!isHighEntropyRequestId(b.request_id)) {
    return badRequest("request_id must be a 256-bit base64url value");
  }
  if (!isNonEmptyBase64(b.batch_signature_b64)) {
    return badRequest("batch_signature_b64 required");
  }
  if (!Array.isArray(b.opks)) return badRequest("opks must be an array");
  if (b.opks.length > MAX_OPK_REPLENISH_BATCH) {
    return badRequest(`opks cannot exceed ${MAX_OPK_REPLENISH_BATCH} entries`);
  }
  if (b.opks.length === 0 && b.spk == null) {
    return badRequest("replenish must include an SPK or at least one OPK");
  }

  const opks: { id: number; pub_b64: string }[] = [];
  const opkIds = new Set<number>();
  for (const raw of b.opks) {
    const o = raw as { id?: unknown; pub_b64?: unknown };
    if (!isU32(o.id)) return badRequest("opk.id must be u32");
    if (!isNonEmptyBase64(o.pub_b64)) return badRequest("opk.pub_b64 must be base64");
    let publicKey: Uint8Array;
    try {
      publicKey = decodeBase64(o.pub_b64);
    } catch {
      return badRequest("opk.pub_b64 must be base64");
    }
    if (publicKey.length !== X25519_PUBLIC_KEY_BYTES) {
      return badRequest("opk.pub_b64 must decode to 32 bytes");
    }
    if (opkIds.has(o.id)) return badRequest("opk ids must be unique within a batch");
    opkIds.add(o.id);
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
    const rotatedAtMs = Date.parse(s.rotated_at);
    if (
      new Date(rotatedAtMs).toISOString() !== s.rotated_at ||
      rotatedAtMs > Date.now() + SIGNED_COMMAND_FRESHNESS_WINDOW_MS
    ) {
      return badRequest("spk.rotated_at must be canonical ISO-8601 and not in the future");
    }
    let publicKey: Uint8Array;
    let signature: Uint8Array;
    try {
      publicKey = decodeBase64(s.pub_b64);
      signature = decodeBase64(s.signature_b64);
    } catch {
      return badRequest("spk fields malformed");
    }
    if (
      publicKey.length !== X25519_PUBLIC_KEY_BYTES ||
      signature.length !== ED25519_SIGNATURE_BYTES
    ) {
      return badRequest("spk public key/signature must decode to 32/64 bytes");
    }
    spk = { pub_b64: s.pub_b64, signature_b64: s.signature_b64, rotated_at: s.rotated_at };
  }

  const user = await getUserForVerify(env.DB, b.user_id);
  if (!user) return notFound("unknown user_id — register before replenish");

  const message = canonicalReplenishBytes({
    user_id: b.user_id,
    timestamp_ms: b.timestamp_ms as number,
    request_id: b.request_id,
    spk,
    opks,
  });
  let ikEd25519: Uint8Array;
  let sig: Uint8Array;
  try {
    ikEd25519 = decodeBase64(user.ik_ed25519_pub);
    sig = decodeBase64(b.batch_signature_b64);
  } catch {
    return unauthorized("batch signature encoding invalid");
  }
  if (ikEd25519.length !== 32 || sig.length !== 64) {
    return unauthorized("batch signature encoding invalid");
  }
  const ok = await verifyEd25519(ikEd25519, message, sig);
  if (!ok) return unauthorized("batch_signature_b64 verification failed");
  if (spk) {
    const validSpkSignature = await verifyEd25519(
      ikEd25519,
      decodeBase64(spk.pub_b64),
      decodeBase64(spk.signature_b64),
    );
    if (!validSpkSignature) return unauthorized("spk signature verification failed");
  }

  const requestDigest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", message),
  );
  try {
    const result = await upsertPrekeyBundleAuthenticated(
      env.DB,
      b.user_id,
      spk,
      opks,
      requestDigest,
      user.ik_ed25519_pub,
      Math.floor(Date.now() / 1000) + 10 * 60,
    );
    if (result === "stale_identity") {
      return unauthorized("identity changed during replenish authorization");
    }
    if (result === "replay") {
      return conflict("signed replenish request already used");
    }
    if (result === "stale_spk") {
      return conflict("SPK rotation must be newer than the current bundle");
    }
    if (result === "missing_spk") {
      return conflict("upload an SPK before an OPK-only replenish");
    }
  } catch (err) {
    if (err instanceof OpkIdConflict) return conflict("opk id already used");
    if (err instanceof OpkPoolQuotaExceeded) return tooMany(60);
    throw err;
  }
  return json({ user_id: b.user_id, opks_added: opks.length });
}
