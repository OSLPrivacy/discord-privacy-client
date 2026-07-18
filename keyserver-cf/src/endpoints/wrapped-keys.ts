import type { Env } from "../env.js";
import {
  canonicalBurnBytes,
  canonicalWrappedKeyPostBytes,
  canonicalWrappedKeyGetBytes,
  CONSUMING_GET_FRESHNESS_WINDOW_MS,
  SIGNED_COMMAND_FRESHNESS_WINDOW_MS,
  WRAPPED_KEY_POST_FRESHNESS_WINDOW_MS,
  type BurnScope,
} from "../lib/canonical.js";
import { verifyEd25519 } from "../lib/crypto.js";
import {
  burnWrappedKeysAuthenticated,
  ConsumingGetReplay,
  ContentIdConflict,
  fetchWrappedKeyAuthenticated,
  getWrappedKeyAccess,
  getUserForVerify,
  insertWrappedKeyAuthenticated,
  StaleSenderIdentity,
  UnknownWrappedKeyRecipient,
  WrappedKeySenderQuotaExceeded,
  WrappedKeyPostReplay,
} from "../lib/db.js";
import {
  badRequest,
  conflict,
  gone,
  json,
  notFound,
  unauthorized,
  tooMany,
} from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import {
  decodeBase64,
  isHighEntropyRequestId,
  isNonEmptyBase64,
  isPositiveInt,
  isProtocolId,
  isU32,
} from "../lib/validation.js";

const ALLOWED_CONTENT_TYPES = new Set(["text", "attachment", "system"]);
const ALLOWED_SYSTEM_KINDS = new Set(["burn-alert"]);
export const MAX_WRAPPED_KEY_LIFETIME_MS = 7 * 24 * 60 * 60 * 1000;
export const MAX_WRAPPED_SHARE_BYTES = 64 * 1024;
const ED25519_SIGNATURE_BYTES = 64;

function decodeExactBase64(value: unknown, expectedBytes: number): Uint8Array | null {
  if (!isNonEmptyBase64(value)) return null;
  try {
    const decoded = decodeBase64(value);
    return decoded.length === expectedBytes ? decoded : null;
  } catch {
    return null;
  }
}

// ---- POST /v1/wrapped-keys ----

export async function handleWrappedKeysPost(
  request: Request,
  env: Env,
): Promise<Response> {
  // One message can fan out a wrapped share to many recipients.
  const rl = await checkRateLimit(env, callerIp(request), 120, "wrapped-post");
  if (!rl.ok) return tooMany(rl.retryAfter);

  let b: Record<string, unknown>;
  try {
    b = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }
  const required = [
    "content_id",
    "content_type",
    "sender_id",
    "recipient_id",
    "session_version",
    "share_index",
    "wrapped_share_blob",
    "blob_version",
    "single_use",
    "expires_at",
    "timestamp_ms",
    "sender_signature_b64",
  ];
  for (const f of required) {
    if (!(f in b)) return badRequest(`missing field: ${f}`);
  }
  if (!isProtocolId(b.content_id)) {
    return badRequest("content_id must be a bounded identifier");
  }
  if (typeof b.content_type !== "string" || !ALLOWED_CONTENT_TYPES.has(b.content_type)) {
    return badRequest(
      `content_type must be one of ${[...ALLOWED_CONTENT_TYPES].join(", ")}`,
    );
  }
  if (b.content_type === "system") {
    if (typeof b.system_message_kind !== "string" || !ALLOWED_SYSTEM_KINDS.has(b.system_message_kind)) {
      return badRequest(
        `system_message_kind must be one of ${[...ALLOWED_SYSTEM_KINDS].join(", ")}`,
      );
    }
  } else if (b.system_message_kind != null) {
    return badRequest("system_message_kind only valid when content_type=system");
  }
  if (!isProtocolId(b.sender_id) || !isProtocolId(b.recipient_id)) {
    return badRequest("sender_id / recipient_id must be bounded identifiers");
  }
  if (!isPositiveInt(b.session_version) || !isU32(b.session_version)) {
    return badRequest("session_version must be a positive integer");
  }
  if (!isU32(b.share_index)) {
    return badRequest("share_index must be a non-negative integer");
  }
  if (!isNonEmptyBase64(b.wrapped_share_blob)) {
    return badRequest("wrapped_share_blob must be base64");
  }
  let wrappedShareBytes: Uint8Array;
  try {
    wrappedShareBytes = decodeBase64(b.wrapped_share_blob);
  } catch {
    return badRequest("wrapped_share_blob must be base64");
  }
  if (wrappedShareBytes.length > MAX_WRAPPED_SHARE_BYTES) {
    return badRequest(`wrapped_share_blob exceeds ${MAX_WRAPPED_SHARE_BYTES} bytes`);
  }
  if (!isPositiveInt(b.blob_version) || !isU32(b.blob_version)) {
    return badRequest("blob_version must be a positive integer");
  }
  if (typeof b.single_use !== "boolean") {
    return badRequest("single_use must be a boolean");
  }
  if (b.single_use && !isU32(b.display_duration_seconds)) {
    return badRequest("display_duration_seconds required when single_use=true");
  }
  if (!b.single_use && b.display_duration_seconds != null) {
    return badRequest("display_duration_seconds only valid when single_use=true");
  }
  if (typeof b.expires_at !== "string") {
    return badRequest("expires_at must be ISO-8601");
  }
  const expiresAt = b.expires_at;
  const expiresAtMs = Date.parse(expiresAt);
  if (
    Number.isNaN(expiresAtMs) ||
    new Date(expiresAtMs).toISOString() !== expiresAt
  ) {
    return badRequest("expires_at must be canonical ISO-8601 UTC");
  }
  if (expiresAtMs <= Date.now()) {
    return badRequest("expires_at must be in the future");
  }
  if (expiresAtMs > Date.now() + MAX_WRAPPED_KEY_LIFETIME_MS) {
    return badRequest("expires_at cannot exceed 7 days");
  }
  if (
    !Number.isSafeInteger(b.timestamp_ms) ||
    (b.timestamp_ms as number) <= 0 ||
    Math.abs(Date.now() - (b.timestamp_ms as number)) >
      WRAPPED_KEY_POST_FRESHNESS_WINDOW_MS
  ) {
    return unauthorized("fresh signed sender authorization required");
  }
  const senderSignature = decodeExactBase64(
    b.sender_signature_b64,
    ED25519_SIGNATURE_BYTES,
  );
  if (!senderSignature) {
    return badRequest("sender_signature_b64 must decode to 64 bytes");
  }

  const sender = await getUserForVerify(env.DB, b.sender_id);
  if (!sender) return unauthorized("sender identity is not registered");
  const message = canonicalWrappedKeyPostBytes({
    content_id: b.content_id,
    content_type: b.content_type,
    system_message_kind: (b.system_message_kind as string | undefined) ?? null,
    sender_id: b.sender_id,
    recipient_id: b.recipient_id,
    session_version: b.session_version as number,
    share_index: b.share_index as number,
    wrapped_share_blob: b.wrapped_share_blob,
    blob_version: b.blob_version as number,
    single_use: b.single_use,
    display_duration_seconds:
      (b.display_duration_seconds as number | undefined) ?? null,
    expires_at: expiresAt,
    timestamp_ms: b.timestamp_ms as number,
  });
  let pubBytes: Uint8Array;
  let sigBytes: Uint8Array;
  try {
    pubBytes = decodeBase64(sender.ik_ed25519_pub);
    sigBytes = senderSignature;
  } catch {
    return unauthorized("signature encoding invalid");
  }
  if (!(await verifyEd25519(pubBytes, message, sigBytes))) {
    return unauthorized("signature verification failed");
  }
  // Check only after authenticating the sender so this route is not an
  // unauthenticated registration oracle. The insert helper repeats this at
  // the D1 boundary to close an unregister race.
  if (!(await getUserForVerify(env.DB, b.recipient_id))) {
    return notFound("wrapped-key recipient is not registered");
  }
  const requestDigest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", message),
  );
  try {
    await insertWrappedKeyAuthenticated(
      env.DB,
      {
        content_id: b.content_id,
        content_type: b.content_type,
        system_message_kind: (b.system_message_kind as string | undefined) ?? null,
        sender_id: b.sender_id,
        recipient_id: b.recipient_id,
        session_version: b.session_version as number,
        share_index: b.share_index as number,
        wrapped_share_blob: b.wrapped_share_blob,
        blob_version: b.blob_version as number,
        single_use: b.single_use ? 1 : 0,
        display_duration_seconds:
          (b.display_duration_seconds as number | undefined) ?? null,
        expires_at: expiresAt,
      },
      requestDigest,
      sender.ik_ed25519_pub,
      Math.floor(Date.now() / 1000) + 10 * 60,
    );
  } catch (err) {
    if (err instanceof ContentIdConflict) return conflict("content_id already exists");
    if (err instanceof WrappedKeyPostReplay) {
      return conflict("signed wrapped-key upload already used");
    }
    if (err instanceof StaleSenderIdentity) {
      return unauthorized("sender identity changed during authorization");
    }
    if (err instanceof UnknownWrappedKeyRecipient) {
      return notFound("wrapped-key recipient is not registered");
    }
    if (err instanceof WrappedKeySenderQuotaExceeded) {
      return tooMany(60);
    }
    throw err;
  }
  return json({ content_id: b.content_id }, { status: 201 });
}

// ---- GET /v1/wrapped-keys/:content_id ----

export async function handleWrappedKeysGet(
  request: Request,
  env: Env,
  contentId: string,
): Promise<Response> {
  if (!isProtocolId(contentId)) return badRequest("content_id must be a bounded identifier");
  const url = new URL(request.url);
  const requesterId = url.searchParams.get("requester_id") ?? "";
  const signedRecipientId = url.searchParams.get("recipient_id") ?? "";
  const timestampMs = Number(url.searchParams.get("ts"));
  const signatureB64 = url.searchParams.get("sig") ?? "";
  if (
    !isProtocolId(requesterId) ||
    !isProtocolId(signedRecipientId) ||
    !Number.isSafeInteger(timestampMs) ||
    timestampMs <= 0 ||
    Math.abs(Date.now() - timestampMs) > CONSUMING_GET_FRESHNESS_WINDOW_MS ||
    decodeExactBase64(signatureB64, ED25519_SIGNATURE_BYTES) === null
  ) {
    return unauthorized("fresh signed recipient authorization required");
  }
  if (requesterId !== signedRecipientId) {
    return unauthorized("only the intended recipient may fetch a wrapped key");
  }

  const requester = await getUserForVerify(env.DB, requesterId);
  if (!requester) return unauthorized("recipient identity is not registered");
  const message = canonicalWrappedKeyGetBytes({
    requester_id: requesterId,
    recipient_id: signedRecipientId,
    content_id: contentId,
    timestamp_ms: timestampMs,
  });
  let pubBytes: Uint8Array;
  let sigBytes: Uint8Array;
  try {
    pubBytes = decodeBase64(requester.ik_ed25519_pub);
    sigBytes = decodeExactBase64(signatureB64, ED25519_SIGNATURE_BYTES)!;
  } catch {
    return unauthorized("signature encoding invalid");
  }
  if (!(await verifyEd25519(pubBytes, message, sigBytes))) {
    return unauthorized("signature verification failed");
  }

  // Resolve the recipient only after a registered requester proves control of
  // the identity key and binds the signature to this exact content ID. Both
  // reusable and single-use wrapped keys require the same fresh authorization.
  const target = await getWrappedKeyAccess(env.DB, contentId);
  if (!target) return notFound("unknown or burned content_id");
  if (target.recipient_id !== signedRecipientId) {
    return unauthorized("signed recipient does not match wrapped-key recipient");
  }

  const requestDigest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", message),
  );
  let result;
  try {
    result = await fetchWrappedKeyAuthenticated(
      env.DB,
      contentId,
      signedRecipientId,
      requestDigest,
      requester.ik_ed25519_pub,
    );
  } catch (err) {
    if (err instanceof ConsumingGetReplay) {
      return conflict("signed wrapped-key request already consumed");
    }
    throw err;
  }
  if (result.status === "not_found") return notFound("unknown or burned content_id");
  if (result.status === "stale_identity") {
    return unauthorized("recipient identity changed during authorization");
  }
  if (result.status === "gone") return gone("tombstoned (past expires_at)");
  return json(result.row);
}

// ---- DELETE /v1/wrapped-keys ----

export async function handleWrappedKeysDelete(
  request: Request,
  env: Env,
): Promise<Response> {
  const rl = await checkRateLimit(env, callerIp(request), 120, "wrapped-delete");
  if (!rl.ok) return tooMany(rl.retryAfter);

  let b: Record<string, unknown>;
  try {
    b = (await request.json()) as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (
    typeof b.scope !== "string" ||
    !["single", "to_user", "all"].includes(b.scope)
  ) {
    return badRequest('scope must be "single" | "to_user" | "all"');
  }
  const scope = b.scope as BurnScope;
  if (!isProtocolId(b.user_id)) return badRequest("user_id must be a bounded identifier");
  if (
    !Number.isSafeInteger(b.timestamp_ms) ||
    (b.timestamp_ms as number) <= 0 ||
    Math.abs(Date.now() - (b.timestamp_ms as number)) >
      SIGNED_COMMAND_FRESHNESS_WINDOW_MS
  ) {
    return unauthorized("fresh signed burn request required");
  }
  if (!isHighEntropyRequestId(b.request_id)) {
    return badRequest("request_id must be a 256-bit base64url value");
  }
  if (!isNonEmptyBase64(b.burn_signature_b64)) {
    return badRequest("burn_signature_b64 required");
  }
  let target: { content_id?: string; user_id?: string } | null;
  if (scope === "single") {
    if (!isProtocolId(b.target_content_id)) {
      return badRequest("target_content_id required for scope=single");
    }
    target = { content_id: b.target_content_id };
  } else if (scope === "to_user") {
    if (!isProtocolId(b.target_user_id)) {
      return badRequest("target_user_id required for scope=to_user");
    }
    target = { user_id: b.target_user_id };
  } else {
    if (b.target_content_id != null || b.target_user_id != null) {
      return badRequest("scope=all rejects target fields");
    }
    target = null;
  }

  const user = await getUserForVerify(env.DB, b.user_id);
  if (!user) return notFound("unknown user_id — register before burn");
  const message = canonicalBurnBytes({
    user_id: b.user_id,
    timestamp_ms: b.timestamp_ms as number,
    request_id: b.request_id,
    scope,
    target: target ?? undefined,
  });
  let ikEd25519: Uint8Array;
  let sig: Uint8Array;
  try {
    ikEd25519 = decodeBase64(user.ik_ed25519_pub);
    sig = decodeBase64(b.burn_signature_b64);
  } catch {
    return unauthorized("burn signature encoding invalid");
  }
  if (ikEd25519.length !== 32 || sig.length !== 64) {
    return unauthorized("burn signature encoding invalid");
  }
  const ok = await verifyEd25519(ikEd25519, message, sig);
  if (!ok) return unauthorized("burn_signature_b64 verification failed");

  const requestDigest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", message),
  );
  const result = await burnWrappedKeysAuthenticated(
    env.DB,
    b.user_id,
    scope,
    target,
    requestDigest,
    user.ik_ed25519_pub,
    Math.floor(Date.now() / 1000) + 10 * 60,
  );
  if (result.status === "stale_identity") {
    return unauthorized("identity changed during burn authorization");
  }
  if (result.status === "replay") {
    return conflict("signed burn request already used");
  }
  return json({ scope, deleted_count: result.deleted_count });
}
