import type { Env } from "../env.js";
import { EXTERNAL_INBOUND_TTL_MS } from "./mailbox.js";
import { encryptExternalMime } from "./external-envelope.js";
import { base64Encode, randomRequestId } from "./protocol.js";

const MAX_EXTERNAL_RAW_BYTES = 512 * 1024;

export async function handleInboundEmail(message: ForwardableEmailMessage, env: Env): Promise<void> {
  const address = message.to.toLowerCase();
  if (address !== message.to || !address.endsWith("@oslprivacy.com")) {
    message.setReject("recipient address is invalid");
    return;
  }
  if (message.rawSize < 1 || message.rawSize > MAX_EXTERNAL_RAW_BYTES) {
    message.setReject("message exceeds the OSL Mail inbound size limit");
    return;
  }
  const recipient = await env.DB.prepare(
    `SELECT a.user_id, u.ik_x25519_pub
       FROM mail_address_epochs a JOIN users u ON u.user_id = a.user_id
      WHERE a.address = ? AND a.state = 'active'`,
  ).bind(address).first<{ user_id: string; ik_x25519_pub: string }>();
  if (!recipient) {
    message.setReject("recipient mailbox is not provisioned");
    return;
  }
  const raw = await readBounded(message.raw, MAX_EXTERNAL_RAW_BYTES);
  if (!raw) {
    message.setReject("message exceeds the OSL Mail inbound size limit");
    return;
  }
  const now = Date.now();
  const expiresAt = now + EXTERNAL_INBOUND_TTL_MS;
  const requestId = randomRequestId();
  const messageId = `mail_${crypto.randomUUID()}`;
  const encrypted = await encryptExternalMime(raw, recipient.ik_x25519_pub, recipient.user_id, messageId, expiresAt);
  const threadMaterial = [message.headers.get("message-id"), message.headers.get("in-reply-to"), message.headers.get("references")]
    .filter((value): value is string => Boolean(value)).join("\n").slice(0, 8192);
  const threadHash = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(threadMaterial || messageId)));
  await env.MAILBOX.getByName(recipient.user_id).store({
    ownerUserId: recipient.user_id,
    messageId,
    requestId,
    kind: "external_envelope",
    senderUserId: null,
    opaqueThreadToken: base64Encode(threadHash).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, ""),
    ciphertextB64: encrypted.ciphertextB64,
    envelopeJson: JSON.stringify(encrypted.envelope),
    recipientKeyFingerprint: encrypted.keyFingerprint,
    receivedAt: now,
    expiresAt,
  });
}

async function readBounded(stream: ReadableStream<Uint8Array>, maxBytes: number): Promise<Uint8Array | null> {
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!value) continue;
    total += value.byteLength;
    if (total > maxBytes) {
      await reader.cancel("message too large");
      return null;
    }
    chunks.push(value);
  }
  const result = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) { result.set(chunk, offset); offset += chunk.byteLength; }
  return result;
}

