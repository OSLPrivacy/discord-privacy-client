import type { Env } from "../env.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { badRequest, conflict, forbidden, json, notFound, serviceUnavailable, tooMany, unauthorized } from "../lib/http.js";
import { isHighEntropyRequestId, isNonEmptyBase64, isProtocolId } from "../lib/validation.js";
import { validNormalizedUsername } from "../lib/username.js";
import { MAIL_MAX_CIPHERTEXT_BYTES, OSL_MAIL_TTL_MS } from "../mail/mailbox.js";
import { authorizeMailRequest, base64Decode, base64Encode, requestDigest } from "../mail/protocol.js";

const DOMAIN = "oslprivacy.com";
const CONTROL_RECEIPT_TTL_SECONDS = 10 * 60;

interface AddressRow {
  address: string;
  username: string;
  user_id: string;
  address_epoch: number;
  state: "active" | "tombstoned";
}

export function handleMailCapabilities(): Response {
  return json({
    version: 1,
    addressDomain: DOMAIN,
    oslToOslE2ee: true,
    externalInbound: true,
    externalInboundEncryption: "plaintext is visible transiently at SMTP ingress, then immediately envelope-encrypted; only ciphertext is retained",
    externalOutbound: false,
    externalOutboundReason: "Cloudflare Email Sending is transactional-only and is not used as a general consumer-mail relay",
    externalOutboundProvider: null,
    retention: {
      oslToOsl: "deleted on authenticated recipient acknowledgement; hard expiry after 7 days",
      externalInbound: "ciphertext deleted on authenticated recipient acknowledgement; hard expiry after 72 hours",
      deliveryReceipts: "non-content receipts expire after 24 hours",
    },
  });
}

export async function handleMailProvision(request: Request, env: Env): Promise<Response> {
  const limited = await identityIngress(request, env, 10, "mail-provision-ip");
  if (limited) return limited;
  const body = await readObject(request);
  if (!body) return badRequest("malformed JSON body");
  const auth = await authorizeMailRequest(env, "PROVISION", body);
  if (!auth) return unauthorized("registered signed identity required");
  if (!validNormalizedUsername(body.username)) return badRequest("username must already be normalized");
  if (typeof body.rotate !== "boolean") return badRequest("rotate must be boolean");
  const username = body.username;
  const address = `${username}@${DOMAIN}`;
  const directory = await env.DB.prepare(
    "SELECT user_id FROM username_directory WHERE username = ?",
  ).bind(username).first<{ user_id: string }>();
  if (!directory || directory.user_id !== auth.userId) return forbidden("current claimed username required");

  const active = await activeAddressForUser(env, auth.userId);
  if (active?.username === username) return json({ ...active, replay: true });
  if (active && !body.rotate) return conflict("mailbox already has an active address; signed rotation required");
  const reserved = await env.DB.prepare(
    "SELECT state FROM mail_address_epochs WHERE address = ?",
  ).bind(address).first<{ state: string }>();
  if (reserved) return conflict("mail address is permanently reserved");

  const digest = await requestDigest(auth.message);
  const nowIso = new Date().toISOString();
  const nowSec = Math.floor(Date.now() / 1000);
  try {
    const results = await env.DB.batch([
      env.DB.prepare("DELETE FROM mail_control_receipts WHERE expires_at <= ?").bind(nowSec),
      env.DB.prepare(
        `INSERT INTO mail_control_receipts(user_id, request_id, operation, request_digest, expires_at)
         VALUES (?, ?, 'provision', ?, ?)`,
      ).bind(auth.userId, auth.requestId, digest, nowSec + CONTROL_RECEIPT_TTL_SECONDS),
      env.DB.prepare(
        `UPDATE mail_address_epochs SET state = 'tombstoned', tombstoned_at = ?
         WHERE user_id = ? AND state = 'active' AND ? = 1`,
      ).bind(nowIso, auth.userId, body.rotate ? 1 : 0),
      env.DB.prepare(
        `INSERT INTO mail_address_epochs(address, username, user_id, address_epoch, state, created_at)
         SELECT ?, ?, ?, COALESCE(MAX(address_epoch), 0) + 1, 'active', ?
         FROM mail_address_epochs WHERE user_id = ?`,
      ).bind(address, username, auth.userId, nowIso, auth.userId),
    ]);
    if ((results[3]?.meta?.changes ?? 0) !== 1) return conflict("mail address could not be provisioned");
  } catch (err) {
    if (/UNIQUE|PRIMARY/i.test(err instanceof Error ? err.message : String(err))) {
      return conflict("request replayed or mail address is permanently reserved");
    }
    throw err;
  }
  const created = await activeAddressForUser(env, auth.userId);
  return json(created, { status: 201 });
}

export async function handleMailConsent(request: Request, env: Env): Promise<Response> {
  const body = await readObject(request);
  if (!body) return badRequest("malformed JSON body");
  const auth = await authorizeMailRequest(env, "CONSENT", body);
  if (!auth) return unauthorized("registered signed identity required");
  if (!isProtocolId(body.sender_user_id) || typeof body.allowed !== "boolean") {
    return badRequest("sender_user_id or allowed invalid");
  }
  const sender = await env.DB.prepare("SELECT 1 ok FROM users WHERE user_id = ?")
    .bind(body.sender_user_id).first<{ ok: number }>();
  if (!sender) return notFound("sender identity not found");
  const receipt = await insertControlReceipt(env, auth, "consent");
  if (!receipt) return conflict("consent request replayed");
  await env.DB.prepare(
    `INSERT INTO mail_sender_consents(recipient_user_id, sender_user_id, allowed, updated_at)
     VALUES (?, ?, ?, ?)
     ON CONFLICT(recipient_user_id, sender_user_id) DO UPDATE SET
       allowed = excluded.allowed, updated_at = excluded.updated_at`,
  ).bind(auth.userId, body.sender_user_id, body.allowed ? 1 : 0, new Date().toISOString()).run();
  return json({ sender_user_id: body.sender_user_id, allowed: body.allowed });
}

export async function handleMailSendOsl(request: Request, env: Env): Promise<Response> {
  const body = await readObject(request);
  if (!body) return badRequest("malformed JSON body");
  const auth = await authorizeMailRequest(env, "SEND-OSL", body);
  if (!auth) return unauthorized("registered signed identity required");
  if (typeof body.recipient_address !== "string" || body.recipient_address !== body.recipient_address.toLowerCase()) {
    return badRequest("recipient_address must already be normalized");
  }
  if (!isNonEmptyBase64(body.ciphertext_b64)) return badRequest("ciphertext_b64 invalid");
  let ciphertextBytes: number;
  try { ciphertextBytes = base64Decode(body.ciphertext_b64).byteLength; }
  catch { return badRequest("ciphertext_b64 invalid"); }
  if (ciphertextBytes < 1 || ciphertextBytes > MAIL_MAX_CIPHERTEXT_BYTES) return badRequest("ciphertext exceeds limit");
  if (typeof body.envelope !== "object" || body.envelope === null || Array.isArray(body.envelope)) return badRequest("envelope invalid");
  const envelopeJson = JSON.stringify(body.envelope);
  if (envelopeJson.length > 8192) return badRequest("envelope too large");
  if (typeof body.opaque_thread_token !== "string" || !/^[A-Za-z0-9_-]{16,128}$/.test(body.opaque_thread_token)) {
    return badRequest("opaque_thread_token invalid");
  }
  if (typeof body.recipient_key_fingerprint !== "string" || body.recipient_key_fingerprint.length > 128) {
    return badRequest("recipient_key_fingerprint invalid");
  }
  const senderAddress = await activeAddressForUser(env, auth.userId);
  if (!senderAddress) return forbidden("active sender mailbox required");
  const recipient = await env.DB.prepare(
    "SELECT address, user_id FROM mail_address_epochs WHERE address = ? AND state = 'active'",
  ).bind(body.recipient_address).first<{ address: string; user_id: string }>();
  if (!recipient) return notFound("recipient mailbox not found");
  if (recipient.user_id !== auth.userId) {
    const consent = await env.DB.prepare(
      "SELECT allowed FROM mail_sender_consents WHERE recipient_user_id = ? AND sender_user_id = ?",
    ).bind(recipient.user_id, auth.userId).first<{ allowed: number }>();
    if (consent?.allowed !== 1) return forbidden("recipient has not allowed this sender");
  }
  const now = Date.now();
  const senderBox = env.MAILBOX.getByName(auth.userId);
  const reservation = await senderBox.reserveOutgoing(auth.userId, auth.requestId, recipient.user_id, ciphertextBytes, now);
  if (!reservation.ok) return reservation.reason?.includes("quota") || reservation.reason?.includes("limit")
    ? tooMany(86_400) : badRequest(reservation.reason ?? "send rejected");
  const messageId = await deterministicMessageId(auth.userId, auth.requestId);
  const recipientBox = env.MAILBOX.getByName(recipient.user_id);
  const stored = await recipientBox.store({
    ownerUserId: recipient.user_id,
    messageId,
    requestId: `delivery:${auth.userId}:${auth.requestId}`,
    kind: "osl_e2ee",
    senderUserId: auth.userId,
    opaqueThreadToken: body.opaque_thread_token,
    ciphertextB64: body.ciphertext_b64,
    envelopeJson,
    recipientKeyFingerprint: body.recipient_key_fingerprint,
    receivedAt: now,
    expiresAt: now + OSL_MAIL_TTL_MS,
  });
  return json({ message_id: messageId, accepted: stored.stored, replay: reservation.replay || stored.replay });
}

export async function handleMailRead(request: Request, env: Env, operation: "LIST" | "FETCH" | "ACK" | "DELETE" | "BURN"): Promise<Response> {
  const body = await readObject(request);
  if (!body) return badRequest("malformed JSON body");
  const auth = await authorizeMailRequest(env, operation, body);
  if (!auth) return unauthorized("registered signed identity required");
  const active = await activeAddressForUser(env, auth.userId);
  if (!active && operation !== "BURN") return notFound("active mailbox not found");
  const box = env.MAILBOX.getByName(auth.userId);
  if (operation === "LIST") {
    const limit = body.limit === undefined ? 50 : body.limit;
    if (typeof limit !== "number" || !Number.isSafeInteger(limit)) return badRequest("limit invalid");
    return json({ messages: await box.list(auth.userId, limit) });
  }
  if (operation === "FETCH") {
    if (!isProtocolId(body.message_id)) return badRequest("message_id invalid");
    const message = await box.fetchMessage(auth.userId, body.message_id);
    return message ? json(message) : notFound("message not found");
  }
  if (operation === "ACK" || operation === "DELETE") {
    if (!isProtocolId(body.message_id)) return badRequest("message_id invalid");
    return json(await box.ack(auth.userId, auth.requestId, body.message_id, Date.now()));
  }
  const result = await box.deleteAll(auth.userId, auth.requestId, Date.now());
  await env.DB.prepare(
    "UPDATE mail_address_epochs SET state = 'tombstoned', tombstoned_at = ? WHERE user_id = ? AND state = 'active'",
  ).bind(new Date().toISOString(), auth.userId).run();
  return json({ ...result, address_tombstoned: true });
}

export function handleMailExternalOutbound(): Response {
  return serviceUnavailable("external outbound is unavailable: Cloudflare Email Sending is transactional-only; OSL Mail requires a mailbox-capable outbound provider");
}

async function identityIngress(request: Request, env: Env, limit: 5 | 10 | 120, scope: string): Promise<Response | null> {
  const result = await checkRateLimit(env, callerIp(request), limit, scope);
  return result.ok ? null : tooMany(result.retryAfter);
}

async function readObject(request: Request): Promise<Record<string, unknown> | null> {
  try {
    const value = await request.json();
    return value !== null && typeof value === "object" && !Array.isArray(value)
      ? value as Record<string, unknown> : null;
  } catch { return null; }
}

async function activeAddressForUser(env: Env, userId: string): Promise<AddressRow | null> {
  return await env.DB.prepare(
    "SELECT address, username, user_id, address_epoch, state FROM mail_address_epochs WHERE user_id = ? AND state = 'active'",
  ).bind(userId).first<AddressRow>();
}

async function insertControlReceipt(
  env: Env,
  auth: { userId: string; requestId: string; message: Uint8Array },
  operation: string,
): Promise<boolean> {
  try {
    const result = await env.DB.prepare(
      `INSERT INTO mail_control_receipts(user_id, request_id, operation, request_digest, expires_at)
       VALUES (?, ?, ?, ?, ?)`,
    ).bind(auth.userId, auth.requestId, operation, await requestDigest(auth.message), Math.floor(Date.now() / 1000) + CONTROL_RECEIPT_TTL_SECONDS).run();
    return (result.meta.changes ?? 0) === 1;
  } catch (err) {
    if (/UNIQUE|PRIMARY/i.test(err instanceof Error ? err.message : String(err))) return false;
    throw err;
  }
}

async function deterministicMessageId(userId: string, requestId: string): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`OSL-MAIL-ID-v1\n${userId}\n${requestId}\n`)));
  return `mail_${base64Encode(digest).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "")}`;
}
