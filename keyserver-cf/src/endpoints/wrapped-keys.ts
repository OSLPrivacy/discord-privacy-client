import type { Env } from "../env.js";
import { checkAdminToken } from "../lib/auth.js";
import { canonicalBurnBytes, type BurnScope } from "../lib/canonical.js";
import { verifyEd25519 } from "../lib/crypto.js";
import {
  burnWrappedKeys,
  ContentIdConflict,
  fetchWrappedKey,
  getUserForVerify,
  insertWrappedKey,
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
  isNonEmptyBase64,
  isNonNegativeInt,
  isPlainString,
  isPositiveInt,
} from "../lib/validation.js";

const ALLOWED_CONTENT_TYPES = new Set(["text", "attachment", "system"]);
const ALLOWED_SYSTEM_KINDS = new Set(["burn-alert"]);

// ---- POST /v1/wrapped-keys ----

export async function handleWrappedKeysPost(
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
  ];
  for (const f of required) {
    if (!(f in b)) return badRequest(`missing field: ${f}`);
  }
  if (!isPlainString(b.content_id)) {
    return badRequest("content_id must be a non-empty string");
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
  if (!isPlainString(b.sender_id) || !isPlainString(b.recipient_id)) {
    return badRequest("sender_id / recipient_id must be non-empty strings");
  }
  if (!isPositiveInt(b.session_version)) {
    return badRequest("session_version must be a positive integer");
  }
  if (!isNonNegativeInt(b.share_index)) {
    return badRequest("share_index must be a non-negative integer");
  }
  if (!isNonEmptyBase64(b.wrapped_share_blob)) {
    return badRequest("wrapped_share_blob must be base64");
  }
  if (!isPositiveInt(b.blob_version)) {
    return badRequest("blob_version must be a positive integer");
  }
  if (typeof b.single_use !== "boolean") {
    return badRequest("single_use must be a boolean");
  }
  if (b.single_use && typeof b.display_duration_seconds !== "number") {
    return badRequest("display_duration_seconds required when single_use=true");
  }
  if (!b.single_use && b.display_duration_seconds != null) {
    return badRequest("display_duration_seconds only valid when single_use=true");
  }
  if (typeof b.expires_at !== "string" || Number.isNaN(Date.parse(b.expires_at))) {
    return badRequest("expires_at must be ISO-8601");
  }
  try {
    await insertWrappedKey(env.DB, {
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
      display_duration_seconds: (b.display_duration_seconds as number | undefined) ?? null,
      expires_at: b.expires_at,
    });
  } catch (err) {
    if (err instanceof ContentIdConflict) return conflict("content_id already exists");
    throw err;
  }
  return json({ content_id: b.content_id }, { status: 201 });
}

// ---- GET /v1/wrapped-keys/:content_id ----

export async function handleWrappedKeysGet(env: Env, contentId: string): Promise<Response> {
  const result = await fetchWrappedKey(env.DB, contentId);
  if (result.status === "not_found") return notFound("unknown or burned content_id");
  if (result.status === "gone") return gone("tombstoned (past expires_at)");
  return json(result.row);
}

// ---- DELETE /v1/wrapped-keys ----

export async function handleWrappedKeysDelete(
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
  if (
    typeof b.scope !== "string" ||
    !["single", "to_user", "all"].includes(b.scope)
  ) {
    return badRequest('scope must be "single" | "to_user" | "all"');
  }
  const scope = b.scope as BurnScope;
  if (!isPlainString(b.user_id)) return badRequest("user_id required");
  if (!isNonEmptyBase64(b.burn_signature_b64)) {
    return badRequest("burn_signature_b64 required");
  }
  let target: { content_id?: string; user_id?: string } | null;
  if (scope === "single") {
    if (!isPlainString(b.target_content_id)) {
      return badRequest("target_content_id required for scope=single");
    }
    target = { content_id: b.target_content_id };
  } else if (scope === "to_user") {
    if (!isPlainString(b.target_user_id)) {
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
    scope,
    target: target ?? undefined,
  });
  const ikEd25519 = decodeBase64(user.ik_ed25519_pub);
  const sig = decodeBase64(b.burn_signature_b64);
  const ok = await verifyEd25519(ikEd25519, message, sig);
  if (!ok) return unauthorized("burn_signature_b64 verification failed");

  const { deleted_count } = await burnWrappedKeys(env.DB, b.user_id, scope, target);
  return json({ scope, deleted_count });
}
