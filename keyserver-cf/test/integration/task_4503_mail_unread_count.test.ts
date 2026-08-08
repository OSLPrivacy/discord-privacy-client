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
      `CREATE TABLE IF NOT EXISTS mail_control_receipts (
        user_id TEXT NOT NULL,
        request_id TEXT NOT NULL,
        operation TEXT NOT NULL,
        request_digest BLOB NOT NULL,
        expires_at INTEGER NOT NULL,
        PRIMARY KEY (user_id, request_id)
      ) WITHOUT ROWID`,
    ),
    env.DB.prepare("DELETE FROM mail_control_receipts"),
    env.DB.prepare("DELETE FROM mail_address_epochs"),
    env.DB.prepare("DELETE FROM users"),
  ]);
});

describe("TASK 4503 OSL Mail unread count", () => {
  it("counts arrived messages, clears exactly the opened message, and survives reopening", async () => {
    const recipient = await createIdentity("task4503-recipient");
    await provisionActiveMailbox(recipient.userId);
    const mailbox = env.MAILBOX.getByName(recipient.userId);
    const now = Date.now();

    for (const index of [1, 2, 3, 4]) {
      await mailbox.store({
        ownerUserId: recipient.userId,
        messageId: `mail_task4503_arrived_${index}`,
        requestId: `delivery-task4503-${index}`,
        kind: "osl_e2ee",
        senderUserId: "task4503-sender",
        opaqueThreadToken: `task4503threadtoken${index}`,
        ciphertextB64: base64Encode(new TextEncoder().encode(`ciphertext ${index}`)),
        envelopeJson: JSON.stringify({ version: 1 }),
        recipientKeyFingerprint: "task4503-fingerprint",
        receivedAt: now + index,
        expiresAt: now + 60_000,
      });
    }

    const beforeOpen = await unreadStatus(recipient);
    expect(beforeOpen).toBe(4);
    console.log("TASK4503 arrived=4 opened=0 unread_count=4");

    const opened = await signedMailRead("FETCH", recipient, { message_id: "mail_task4503_arrived_1" });
    expect(opened.status).toBe(200);
    expect((await opened.json() as { message_id: string }).message_id).toBe("mail_task4503_arrived_1");
    const afterOpen = await unreadStatus(recipient);
    expect(afterOpen).toBe(3);
    console.log("TASK4503 arrived=4 opened=1 unread_count=3");

    // A fresh client handle reads the Durable Object's stored marker rather
    // than retaining the earlier process's count in memory.
    const reopenedAppMailbox = env.MAILBOX.getByName(recipient.userId);
    expect(await reopenedAppMailbox.unreadCount(recipient.userId)).toBe(3);
    const afterReopen = await unreadStatus(recipient);
    expect(afterReopen).toBe(3);
    console.log("TASK4503 reopened_app=true unread_count=3");
  });
});

async function unreadStatus(identity: Identity): Promise<number> {
  const response = await signedMailRead("LIST", identity, { limit: 1 });
  expect(response.status).toBe(200);
  const body = await response.json() as { unread_count: number };
  return body.unread_count;
}

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

async function provisionActiveMailbox(userId: string): Promise<void> {
  await env.DB.prepare(
    `INSERT INTO mail_address_epochs(address, username, user_id, address_epoch, state, created_at)
     VALUES (?, ?, ?, 1, 'active', ?)`,
  ).bind("task4503@oslprivacy.com", "task4503", userId, new Date().toISOString()).run();
}

async function signedMailRead(
  operation: "LIST" | "FETCH",
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
