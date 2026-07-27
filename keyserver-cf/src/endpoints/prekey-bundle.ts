import type { Env } from "../env.js";
import {
  canonicalPrekeyBundleGetBytes,
  canonicalReplenishBytes,
  CONSUMING_GET_FRESHNESS_WINDOW_MS,
  SIGNED_COMMAND_FRESHNESS_WINDOW_MS,
} from "../lib/canonical.js";
import { verifyEd25519 } from "../lib/crypto.js";
import {
  getPrekeyBundleSpk,
  getScheme1PrekeyContext,
  getScheme1ReplenishReceipt,
  getSignedIdentity,
  getUserForVerify,
  OpkIdConflict,
  OpkPoolQuotaExceeded,
  ConsumingGetReplay,
  popPrekeyBundleAuthenticated,
  upsertScheme1PrekeyBundleAuthenticated,
  upsertPrekeyBundleAuthenticated,
} from "../lib/db.js";
import {
  canonicalReplenishV2Bytes,
  OPK_LIFECYCLE_VERSION,
  OPK_OWNER_PROOF_VERSION,
  parseOpkOwnerProof,
  SCHEME1_PREKEY_PROTOCOL_VERSION,
  scheme1ReplenishResponse,
  validateScheme1OwnerProofBatch,
  type ReplenishOpkV2,
  type ReplenishSpkV2,
  type Scheme1IdentityAuthority,
} from "../lib/prekey-owner-proof.js";
import {
  CANONICAL_IDENTITY_BUNDLE_VERSION,
  decodeCanonicalBase64,
  decodeCanonicalEd25519SignatureBytes,
} from "../lib/identity-authority.js";
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
  if (result.status === "invalid_scheme1_state") {
    return conflict("scheme-1 prekey lifecycle is absent or stale");
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

  let parsed: unknown;
  try {
    parsed = await request.json();
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return badRequest("replenish body must be an object");
  }
  const b = parsed as Record<string, unknown>;
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

  const signedIdentity = await getSignedIdentity(env.DB, b.user_id);
  if (!signedIdentity) return notFound("unknown user_id — register before replenish");
  if (signedIdentity.identity_scheme === 1) {
    if (
      !signedIdentity.ik_root_ed25519_pub ||
      !signedIdentity.identity_bundle_proof_sig
    ) {
      return conflict("scheme-1 identity authority is incomplete");
    }
    return await handleScheme1PrekeyReplenish(
      b,
      env,
      {
        user_id: signedIdentity.user_id,
        identity_scheme: 1,
        identity_bundle_version: 1,
        identity_revision: signedIdentity.identity_revision,
        ik_root_ed25519_pub: signedIdentity.ik_root_ed25519_pub,
        ik_x25519_pub: signedIdentity.ik_x25519_pub,
        ik_ed25519_pub: signedIdentity.ik_ed25519_pub,
        ik_mlkem768_pub: signedIdentity.ik_mlkem768_pub,
        ik_ratchet_initial_pub: signedIdentity.ik_ratchet_initial_pub,
        rn_capabilities: signedIdentity.rn_capabilities,
        identity_bundle_proof_sig:
          signedIdentity.identity_bundle_proof_sig,
        registration_sig: signedIdentity.ik_x25519_signature,
      },
    );
  }
  if (b.protocol_version !== undefined && b.protocol_version !== 1) {
    return badRequest("legacy identities require explicit protocol_version 1");
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
    ikEd25519 = decodeBase64(signedIdentity.ik_ed25519_pub);
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
      signedIdentity.ik_ed25519_pub,
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

function parseScheme1Spk(value: unknown): ReplenishSpkV2 | null {
  if (value === null || value === undefined) return null;
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("spk fields malformed");
  }
  const raw = value as Record<string, unknown>;
  const keys = Object.keys(raw).sort();
  if (
    keys.join(",") !==
      ["pub_b64", "rotated_at", "signature_b64"].sort().join(",") ||
    typeof raw.pub_b64 !== "string" ||
    typeof raw.signature_b64 !== "string" ||
    typeof raw.rotated_at !== "string"
  ) {
    throw new Error("spk fields are noncanonical");
  }
  decodeCanonicalBase64(raw.pub_b64, 32, "SPK public key");
  decodeCanonicalEd25519SignatureBytes(
    raw.signature_b64,
    "SPK signature",
  );
  const rotatedAt = Date.parse(raw.rotated_at);
  if (
    Number.isNaN(rotatedAt) ||
    new Date(rotatedAt).toISOString() !== raw.rotated_at ||
    rotatedAt > Date.now() + SIGNED_COMMAND_FRESHNESS_WINDOW_MS
  ) {
    throw new Error("spk.rotated_at must be canonical ISO-8601 and not in the future");
  }
  return {
    pub_b64: raw.pub_b64,
    signature_b64: raw.signature_b64,
    rotated_at: raw.rotated_at,
  };
}

function scheme1LifecycleMatches(args: {
  context: Awaited<ReturnType<typeof getScheme1PrekeyContext>>;
  identity: Scheme1IdentityAuthority;
  spk: ReplenishSpkV2;
  identity_commitment_b64: string;
  lifecycle_generation: number;
  batch_commitment_b64: string;
}): boolean {
  const context = args.context;
  return context !== null &&
    context.proof_version === OPK_OWNER_PROOF_VERSION &&
    context.identity_bundle_version === CANONICAL_IDENTITY_BUNDLE_VERSION &&
    context.lifecycle_version === OPK_LIFECYCLE_VERSION &&
    context.user_id === args.identity.user_id &&
    context.ik_root_ed25519_pub === args.identity.ik_root_ed25519_pub &&
    context.ik_ed25519_pub === args.identity.ik_ed25519_pub &&
    context.identity_revision === args.identity.identity_revision &&
    context.identity_bundle_commitment_b64 ===
      args.identity_commitment_b64 &&
    context.rn_capabilities === args.identity.rn_capabilities &&
    context.spk_pub_b64 === args.spk.pub_b64 &&
    context.spk_signature_b64 === args.spk.signature_b64 &&
    context.spk_rotated_at === args.spk.rotated_at &&
    context.highest_generation === args.lifecycle_generation &&
    context.batch_commitment_b64 === args.batch_commitment_b64;
}

function scheme1ReceiptMatches(args: {
  receipt: Awaited<ReturnType<typeof getScheme1ReplenishReceipt>>;
  identity: Scheme1IdentityAuthority;
  identity_commitment_b64: string;
  lifecycle_generation: number;
  batch_commitment_b64: string;
  opks_added: number;
}): boolean {
  const receipt = args.receipt;
  return receipt !== null &&
    receipt.identity_scheme === 1 &&
    receipt.protocol_version === SCHEME1_PREKEY_PROTOCOL_VERSION &&
    receipt.identity_revision === args.identity.identity_revision &&
    receipt.identity_bundle_commitment_b64 ===
      args.identity_commitment_b64 &&
    receipt.lifecycle_generation === args.lifecycle_generation &&
    receipt.batch_commitment_b64 === args.batch_commitment_b64 &&
    receipt.opks_added === args.opks_added;
}

async function handleScheme1PrekeyReplenish(
  body: Record<string, unknown>,
  env: Env,
  identity: Scheme1IdentityAuthority,
): Promise<Response> {
  if (body.protocol_version !== 2) {
    return badRequest(
      "scheme-1 identities require protocol_version 2 owner proofs",
    );
  }
  const exactTopLevel = [
    "protocol_version",
    "user_id",
    "timestamp_ms",
    "request_id",
    "batch_signature_b64",
    "spk",
    "opks",
  ].sort();
  if (
    Object.keys(body).sort().some(
      (key, index) => key !== exactTopLevel[index],
    ) ||
    Object.keys(body).length !== exactTopLevel.length
  ) {
    return badRequest("scheme-1 replenish fields are noncanonical");
  }
  if (
    identity.identity_scheme !== 1 ||
    !identity.ik_root_ed25519_pub ||
    !identity.identity_bundle_proof_sig ||
    identity.identity_revision < 1
  ) {
    return conflict("scheme-1 identity authority is incomplete");
  }

  let suppliedSpk: ReplenishSpkV2 | null;
  const opks: ReplenishOpkV2[] = [];
  try {
    suppliedSpk = parseScheme1Spk(body.spk);
    for (const value of body.opks as unknown[]) {
      if (!value || typeof value !== "object" || Array.isArray(value)) {
        throw new Error("opk must be an object");
      }
      const raw = value as Record<string, unknown>;
      if (
        Object.keys(raw).sort().join(",") !==
          ["id", "owner_proof", "pub_b64"].sort().join(",") ||
        !isU32(raw.id) ||
        typeof raw.pub_b64 !== "string"
      ) {
        throw new Error("scheme-1 OPK fields are noncanonical");
      }
      decodeCanonicalBase64(raw.pub_b64, 32, "OPK public key");
      opks.push({
        id: raw.id,
        pub_b64: raw.pub_b64,
        owner_proof: parseOpkOwnerProof(raw.owner_proof),
      });
    }
  } catch (error) {
    return badRequest(
      error instanceof Error ? error.message : "scheme-1 proof is malformed",
    );
  }
  if (opks.length === 0) {
    return badRequest("scheme-1 replenish requires a nonempty OPK proof batch");
  }

  const [currentSpk, currentContext] = await Promise.all([
    getPrekeyBundleSpk(env.DB, identity.user_id),
    getScheme1PrekeyContext(env.DB, identity.user_id),
  ]);
  if (!suppliedSpk && !currentSpk) {
    return conflict("scheme-1 lifecycle genesis must include an SPK");
  }
  const resolvedSpk = suppliedSpk ?? currentSpk!;
  const firstProof = opks[0]!.owner_proof;
  if (
    firstProof.spk_pub_b64 !== resolvedSpk.pub_b64 ||
    firstProof.spk_signature_b64 !== resolvedSpk.signature_b64 ||
    firstProof.spk_rotated_at !== resolvedSpk.rotated_at
  ) {
    return badRequest("OPK owner proof is bound to a different SPK");
  }
  let proofState;
  let message: Uint8Array;
  let currentEd25519: Uint8Array;
  let batchSignature: Uint8Array;
  try {
    proofState = await validateScheme1OwnerProofBatch({
      identity,
      spk: resolvedSpk,
      opks,
    });
    message = canonicalReplenishV2Bytes({
      user_id: identity.user_id,
      timestamp_ms: body.timestamp_ms as number,
      request_id: body.request_id as string,
      spk: suppliedSpk,
      opks,
    });
    currentEd25519 = decodeCanonicalBase64(
      identity.ik_ed25519_pub,
      32,
      "current Ed25519 identity key",
    );
    batchSignature = decodeCanonicalEd25519SignatureBytes(
      body.batch_signature_b64,
      "batch signature",
    );
  } catch (error) {
    return badRequest(
      error instanceof Error ? error.message : "scheme-1 proof is invalid",
    );
  }
  if (
    !(await verifyEd25519(currentEd25519, message, batchSignature))
  ) {
    return unauthorized("batch_signature_b64 verification failed");
  }
  if (
    !(await verifyEd25519(
      currentEd25519,
      decodeCanonicalBase64(resolvedSpk.pub_b64, 32, "SPK public key"),
      decodeCanonicalEd25519SignatureBytes(
        resolvedSpk.signature_b64,
        "SPK signature",
      ),
    ))
  ) {
    return unauthorized("spk signature verification failed");
  }

  const requestDigest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", message),
  );
  const stableResponse = scheme1ReplenishResponse({
    user_id: identity.user_id,
    lifecycle_generation: proofState.lifecycle_generation,
    batch_commitment_b64: proofState.batch_commitment_b64,
    opks_added: opks.length,
  });
  const receipt = await getScheme1ReplenishReceipt(
    env.DB,
    identity.user_id,
    identity.ik_ed25519_pub,
    requestDigest,
  );
  if (receipt) {
    if (!scheme1ReceiptMatches({
      receipt,
      identity,
      identity_commitment_b64: proofState.identity_bundle_commitment_b64,
      lifecycle_generation: proofState.lifecycle_generation,
      batch_commitment_b64: proofState.batch_commitment_b64,
      opks_added: opks.length,
    })) {
      return conflict("scheme-1 replenish replay receipt context mismatch");
    }
    return json(stableResponse);
  }

  const identityChanged = currentContext !== null && (
    currentContext.identity_revision !== identity.identity_revision ||
    currentContext.ik_ed25519_pub !== identity.ik_ed25519_pub ||
    currentContext.identity_bundle_commitment_b64 !==
      proofState.identity_bundle_commitment_b64 ||
    currentContext.rn_capabilities !== identity.rn_capabilities
  );
  if (identityChanged && !suppliedSpk) {
    return conflict(
      "identity revision requires an SPK-bound replacement proof batch",
    );
  }
  const spkChanged = currentSpk !== null && (
    currentSpk.pub_b64 !== resolvedSpk.pub_b64 ||
    currentSpk.signature_b64 !== resolvedSpk.signature_b64 ||
    currentSpk.rotated_at !== resolvedSpk.rotated_at
  );
  if (
    spkChanged &&
    currentSpk &&
    Date.parse(resolvedSpk.rotated_at) <= Date.parse(currentSpk.rotated_at)
  ) {
    return conflict("SPK rotation must be newer than the current bundle");
  }

  if (!currentContext && firstProof.lifecycle_generation !== 1) {
    return conflict("scheme-1 lifecycle genesis generation must be 1");
  }
  if (currentContext) {
    if (scheme1LifecycleMatches({
      context: currentContext,
      identity,
      spk: resolvedSpk,
      identity_commitment_b64: proofState.identity_bundle_commitment_b64,
      lifecycle_generation: proofState.lifecycle_generation,
      batch_commitment_b64: proofState.batch_commitment_b64,
    })) {
      // A fresh, valid request for the exact already-committed generation and
      // batch is an authenticated lifecycle readback, not a CAS failure.
      return json(stableResponse);
    }
    if (
      firstProof.lifecycle_generation !==
        currentContext.highest_generation + 1
    ) {
      return conflict("scheme-1 prekey lifecycle CAS is stale");
    }
  }

  try {
    const result = await upsertScheme1PrekeyBundleAuthenticated(
      env.DB,
      {
        user_id: identity.user_id,
        ik_root_ed25519_pub: identity.ik_root_ed25519_pub,
        ik_ed25519_pub: identity.ik_ed25519_pub,
        identity_revision: identity.identity_revision,
        identity_bundle_proof_sig: identity.identity_bundle_proof_sig,
        identity_bundle_commitment_b64:
          proofState.identity_bundle_commitment_b64,
        rn_capabilities: identity.rn_capabilities,
        lifecycle_generation: proofState.lifecycle_generation,
        batch_commitment_b64: proofState.batch_commitment_b64,
      },
      resolvedSpk,
      suppliedSpk !== null,
      identityChanged || spkChanged,
      opks.map((opk) => ({
        id: opk.id,
        pub_b64: opk.pub_b64,
        owner_proof_json: JSON.stringify(opk.owner_proof),
      })),
      requestDigest,
      Math.floor(Date.now() / 1000) + 10 * 60,
    );
    if (result === "stale_identity") {
      return unauthorized("identity changed during replenish authorization");
    }
    if (result === "replay") {
      const replayReceipt = await getScheme1ReplenishReceipt(
        env.DB,
        identity.user_id,
        identity.ik_ed25519_pub,
        requestDigest,
      );
      if (scheme1ReceiptMatches({
        receipt: replayReceipt,
        identity,
        identity_commitment_b64: proofState.identity_bundle_commitment_b64,
        lifecycle_generation: proofState.lifecycle_generation,
        batch_commitment_b64: proofState.batch_commitment_b64,
        opks_added: opks.length,
      })) {
        return json(stableResponse);
      }
      return conflict("scheme-1 replenish replay receipt context mismatch");
    }
    if (result === "stale_lifecycle") {
      const committed = await getScheme1PrekeyContext(
        env.DB,
        identity.user_id,
      );
      if (scheme1LifecycleMatches({
        context: committed,
        identity,
        spk: resolvedSpk,
        identity_commitment_b64: proofState.identity_bundle_commitment_b64,
        lifecycle_generation: proofState.lifecycle_generation,
        batch_commitment_b64: proofState.batch_commitment_b64,
      })) {
        return json(stableResponse);
      }
      return conflict("scheme-1 prekey lifecycle CAS is stale");
    }
  } catch (error) {
    if (error instanceof OpkIdConflict) {
      return conflict("opk id or public key already used");
    }
    if (error instanceof OpkPoolQuotaExceeded) return tooMany(60);
    throw error;
  }
  return json(stableResponse);
}
