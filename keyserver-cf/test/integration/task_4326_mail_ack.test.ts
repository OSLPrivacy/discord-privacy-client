import { beforeEach, describe, expect, it } from "vitest";
import { env } from "cloudflare:test";
import { handleMailRead } from "../../src/endpoints/mail.js";
import { base64Encode, mailSignedMessage, randomRequestId } from "../../src/mail/protocol.js";

interface Identity {
  userId: string;
  signingKey: CryptoKey;
}

beforeEach(async () => {
  await env.DB.batch([
    env.DB.prepare(
      `CREATE TABLE IF NOT EXISTS users (
        user_id TEXT PRIMARY KEY,
        ik_x25519_pub TEXT NOT NULL,
        ik_ed25519_pub TEXT NOT NULL,
        ik_mlkem768_pub TEXT NOT NULL,
        ik_x25519_signature TEXT NOT NULL,
        registered_at TEXT NOT NULL,
        ik_ratchet_initial_pub TEXT,
        identity_lookup_enabled INTEGER NOT NULL DEFAULT 1
      ) WITHOUT ROWID`,
    ),
    env.DB.prepare(
      `CREATE TABLE IF NOT EXISTS mail_address_epochs (
        address TEXT PRIMARY KEY,
        username TEXT NOT NULL,
        user_id TEXT NOT NULL,
        address_epoch INTEGER NOT NULL,
        state TEXT NOT NULL CHECK (state IN ('active', 'tombstoned')),
        created_at TEXT NOT NULL,
        tombstoned_at TEXT,
        UNIQUE (user_id, address_epoch)
      ) WITHOUT ROWID`,
    ),
    env.DB.prepare(
      `CREATE UNIQUE INDEX IF NOT EXISTS idx_mail_one_active_address
        ON mail_address_epochs (user_id) WHERE state = 'active'`,
    ),
    env.DB.prepare(
      `CREATE TABLE IF NOT EXISTS mail_sender_consents (
        recipient_user_id TEXT NOT NULL,
        sender_user_id TEXT NOT NULL,
        allowed INTEGER NOT NULL CHECK (allowed IN (0, 1)),
        updated_at TEXT NOT NULL,
        PRIMARY KEY (recipient_user_id, sender_user_id)
      ) WITHOUT ROWID`,
    ),
    env.DB.prepare(
      `CREATE TABLE IF NOT EXISTS mail_control_receipts (
        user_id TEXT NOT NULL,
        request_id TEXT NOT NULL,
        operation TEXT NOT NULL,
        request_digest BLOB NOT NULL,
        expires_at INTEGER NOT NULL,
        PRIMARY KEY (user_id, request_id)
      ) WITHOUT ROWID`,
    ),
  ]);
  await env.DB.batch([
    env.DB.prepare("DELETE FROM mail_control_receipts"),
    env.DB.prepare("DELETE FROM mail_sender_consents"),
    env.DB.prepare("DELETE FROM mail_address_epochs"),
    env.DB.prepare("DELETE FROM users"),
  ]);
});

describe("task 4326 OSL Mail I-have-taken-it command", () => {
  it("acknowledges only opened messages and keeps the unacknowledged-drop count at zero", async () => {
    const bob = await createIdentity("bob-task4326-id");
    await provisionActiveMailbox(bob.userId, "bob_task4326");

    const box = env.MAILBOX.getByName(bob.userId);
    const now = Date.now();
    const openedMessageIds = [
      "mail_task4326_opened_1",
      "mail_task4326_opened_2",
      "mail_task4326_opened_3",
    ];
    const unopenedMessageId = "mail_task4326_unopened";
    for (const messageId of [...openedMessageIds, unopenedMessageId]) {
      await box.store({
        ownerUserId: bob.userId,
        messageId,
        requestId: `delivery:${messageId}`,
        kind: "osl_e2ee",
        senderUserId: "sender-task4326",
        opaqueThreadToken: `threadtoken${messageId.replaceAll(/[^A-Za-z0-9]/g, "").slice(0, 24)}`,
        ciphertextB64: base64Encode(new TextEncoder().encode(`ciphertext for ${messageId}`)),
        envelopeJson: JSON.stringify({ version: 1, nonce_b64: "bm9uY2U=" }),
        recipientKeyFingerprint: `fingerprint-${messageId}`,
        receivedAt: now,
        expiresAt: now + 60_000,
      });
    }

    for (const messageId of openedMessageIds) {
      const opened = await signedMailRead("FETCH", bob, { message_id: messageId });
      expect(opened.status).toBe(200);
    }

    let acknowledged = 0;
    for (const messageId of openedMessageIds) {
      const acked = await signedMailRead("ACK", bob, { message_id: messageId });
      expect(acked.status).toBe(200);
      expect(await acked.json()).toMatchObject({ deleted: true, replay: false });
      acknowledged += 1;
    }

    const listAfterAck = await signedMailRead("LIST", bob, { limit: 10 }).then((response) => response.json()) as {
      messages: Array<{ message_id: string }>;
    };
    const heldOpenedMessages = listAfterAck.messages
      .filter((message) => openedMessageIds.includes(message.message_id))
      .length;

    const unopenedAck = await signedMailRead("ACK", bob, { message_id: unopenedMessageId });
    expect(unopenedAck.status).toBe(409);
    const refusal = await unopenedAck.json() as { error: string };
    expect(refusal.error).toBe(`message was never opened: ${unopenedMessageId}`);

    const stats = await box.retentionStats(bob.userId);
    expect(stats.dropped_without_acknowledgement).toBe(0);
    console.log(`task_4326_opened_messages_acknowledged=${acknowledged}`);
    console.log(`task_4326_opened_messages_held_after_ack=${heldOpenedMessages}`);
    console.log(`task_4326_unopened_refusal="${refusal.error}"`);
    console.log(`task_4326_dropped_without_acknowledgement=${stats.dropped_without_acknowledgement}`);
  });
});

async function createIdentity(userId: string): Promise<Identity> {
  const ed = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]) as CryptoKeyPair;
  const edRaw = await crypto.subtle.exportKey("raw", ed.publicKey) as ArrayBuffer;
  const x = await crypto.subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]) as CryptoKeyPair;
  const xRaw = await crypto.subtle.exportKey("raw", x.publicKey) as ArrayBuffer;
  await env.DB.prepare(
    `INSERT INTO users(user_id,ik_x25519_pub,ik_ed25519_pub,ik_mlkem768_pub,ik_x25519_signature,registered_at,ik_ratchet_initial_pub,identity_lookup_enabled)
     VALUES (?,?,?,?,?,?,?,1)`,
  ).bind(
    userId,
    base64Encode(new Uint8Array(xRaw)),
    base64Encode(new Uint8Array(edRaw)),
    "mlkem",
    "signature",
    new Date().toISOString(),
    null,
  ).run();
  return { userId, signingKey: ed.privateKey };
}

async function provisionActiveMailbox(userId: string, username: string): Promise<void> {
  await env.DB.prepare(
    `INSERT INTO mail_address_epochs(address, username, user_id, address_epoch, state, created_at)
     VALUES (?, ?, ?, 1, 'active', ?)`,
  ).bind(`${username}@oslprivacy.com`, username, userId, new Date().toISOString()).run();
}

async function signedMailRead(
  operation: "LIST" | "FETCH" | "ACK",
  identity: Identity,
  fields: Record<string, unknown>,
): Promise<Response> {
  const body: Record<string, unknown> = {
    user_id: identity.userId,
    request_id: randomRequestId(),
    timestamp_ms: Date.now(),
    ...fields,
  };
  const signature = await crypto.subtle.sign({ name: "Ed25519" }, identity.signingKey, mailSignedMessage(operation, body));
  body.signature_b64 = base64Encode(new Uint8Array(signature));
  return await handleMailRead(
    new Request("https://test/v1/mail/read", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    }),
    env,
    operation,
  );
}
