import { describe, expect, it, vi } from "vitest";

vi.mock("../src/lib/username.js", () => ({
  validNormalizedUsername: (value: unknown): value is string =>
    typeof value === "string" && /^[a-z0-9](?:[a-z0-9_]{1,28}[a-z0-9])?$/.test(value),
}));
vi.mock("../src/mail/mailbox.js", () => ({
  MAIL_MAX_CIPHERTEXT_BYTES: 768 * 1024,
  OSL_MAIL_TTL_MS: 7 * 24 * 60 * 60 * 1000,
}));

import type { Env } from "../src/env.js";
import { handleMailConsent, handleMailRead, handleMailSendOsl } from "../src/endpoints/mail.js";
import { base64Encode, mailSignedMessage, randomRequestId } from "../src/mail/protocol.js";

interface Identity {
  userId: string;
  username: string;
  address: string;
  signingKey: CryptoKey;
  publicKeyB64: string;
}

interface MessageRecord {
  message_id: string;
  kind: string;
  sender_user_id: string | null;
  opaque_thread_token: string;
  ciphertext_b64: string;
  envelope_json: string;
  recipient_key_fingerprint: string;
}

describe("task 4504 OSL Mail agreement command", () => {
  it("refuses, accepts after named agreement, and refuses after removal", async () => {
    const alice = await identity("alice-4504-user", "alice_4504");
    const bob = await identity("bob-4504-user", "bob_4504");
    const env = fakeEnv([alice, bob]);

    const refusedBefore = await signedPost(env, "/v1/mail/send/osl", "SEND-OSL", alice, oslPayload(bob.address, "task4504 before agreement"));
    const listBefore = await signedPost(env, "/v1/mail/list", "LIST", bob, { limit: 10 }).then((response) => response.json()) as {
      messages: MessageRecord[];
    };
    println4504("NO_AGREEMENT_SEND_STATUS", refusedBefore.status);
    println4504("MESSAGES_AFTER_NO_AGREEMENT", listBefore.messages.length);
    expect(refusedBefore.status).toBe(403);
    expect(listBefore.messages).toHaveLength(0);

    const agreement = await signedPost(env, "/v1/mail/consent", "CONSENT", bob, { sender_username: alice.username, allowed: true });
    const agreementBody = await agreement.json() as {
      sender_user_id: string;
      sender_username: string;
      sender_address: string;
      allowed: boolean;
    };
    println4504("AGREEMENT_COMMAND_STATUS", `${agreement.status} sender_username=${agreementBody.sender_username} allowed=${agreementBody.allowed}`);
    expect(agreement.status).toBe(200);
    expect(agreementBody).toEqual({
      sender_user_id: alice.userId,
      sender_username: alice.username,
      sender_address: alice.address,
      allowed: true,
    });

    const accepted = await signedPost(env, "/v1/mail/send/osl", "SEND-OSL", alice, oslPayload(bob.address, "task4504 after agreement"));
    const acceptedBody = await accepted.json() as { accepted: boolean };
    const listAfterAgreement = await signedPost(env, "/v1/mail/list", "LIST", bob, { limit: 10 }).then((response) => response.json()) as {
      messages: MessageRecord[];
    };
    println4504("AFTER_AGREEMENT_SEND_STATUS", `${accepted.status} accepted=${acceptedBody.accepted}`);
    println4504("MESSAGES_AFTER_AGREEMENT", listAfterAgreement.messages.length);
    expect(accepted.status).toBe(200);
    expect(acceptedBody.accepted).toBe(true);
    expect(listAfterAgreement.messages).toHaveLength(1);

    const removal = await signedPost(env, "/v1/mail/consent", "CONSENT", bob, { sender_username: alice.username, allowed: false });
    const removalBody = await removal.json() as { allowed: boolean };
    println4504("REMOVE_AGREEMENT_STATUS", `${removal.status} allowed=${removalBody.allowed}`);
    expect(removal.status).toBe(200);
    expect(removalBody.allowed).toBe(false);

    const refusedAfterRemoval = await signedPost(env, "/v1/mail/send/osl", "SEND-OSL", alice, oslPayload(bob.address, "task4504 after removal"));
    const listAfterRemoval = await signedPost(env, "/v1/mail/list", "LIST", bob, { limit: 10 }).then((response) => response.json()) as {
      messages: MessageRecord[];
    };
    println4504("AFTER_REMOVE_SEND_STATUS", refusedAfterRemoval.status);
    println4504("MESSAGES_AFTER_REMOVE", listAfterRemoval.messages.length);
    expect(refusedAfterRemoval.status).toBe(403);
    expect(listAfterRemoval.messages).toHaveLength(1);
  });
});

async function identity(userId: string, username: string): Promise<Identity> {
  const pair = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  const publicKeyB64 = base64Encode(new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey)));
  return {
    userId,
    username,
    address: `${username}@oslprivacy.com`,
    signingKey: pair.privateKey,
    publicKeyB64,
  };
}

async function signedPost(
  env: Env,
  path: "/v1/mail/consent" | "/v1/mail/list" | "/v1/mail/send/osl",
  operation: "CONSENT" | "LIST" | "SEND-OSL",
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
  const request = new Request(`https://test${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (path === "/v1/mail/consent") return await handleMailConsent(request, env);
  if (path === "/v1/mail/list") return await handleMailRead(request, env, "LIST");
  return await handleMailSendOsl(request, env);
}

function oslPayload(recipientAddress: string, plaintextMarker: string): Record<string, unknown> {
  return {
    recipient_address: recipientAddress,
    opaque_thread_token: `thread${base64Encode(new TextEncoder().encode(plaintextMarker)).replaceAll(/[^A-Za-z0-9]/g, "").slice(0, 24)}`,
    ciphertext_b64: base64Encode(new TextEncoder().encode(plaintextMarker)),
    envelope: { version: 1, nonce_b64: "bm9uY2U=" },
    recipient_key_fingerprint: `fingerprint-${plaintextMarker.replaceAll(/[^a-z0-9]/gi, "-").toLowerCase()}`,
  };
}

function fakeEnv(identities: Identity[]): Env {
  const users = new Map(identities.map((id) => [id.userId, id]));
  const byAddress = new Map(identities.map((id) => [id.address, id]));
  const byUsername = new Map(identities.map((id) => [id.username, id]));
  const consents = new Map<string, number>();
  const receipts = new Set<string>();
  const boxes = new Map<string, FakeMailbox>();
  const db = {
    prepare(sql: string) {
      return new FakeStatement(sql, users, byAddress, byUsername, consents, receipts);
    },
  };
  const mailbox = {
    getByName(name: string) {
      let box = boxes.get(name);
      if (!box) {
        box = new FakeMailbox();
        boxes.set(name, box);
      }
      return box;
    },
  };
  return { DB: db, MAILBOX: mailbox } as unknown as Env;
}

class FakeStatement {
  private args: unknown[] = [];

  constructor(
    private readonly sql: string,
    private readonly users: Map<string, Identity>,
    private readonly byAddress: Map<string, Identity>,
    private readonly byUsername: Map<string, Identity>,
    private readonly consents: Map<string, number>,
    private readonly receipts: Set<string>,
  ) {}

  bind(...args: unknown[]): this {
    this.args = args;
    return this;
  }

  async first<T>(): Promise<T | null> {
    if (this.sql.includes("FROM users") && this.sql.includes("ik_ed25519_pub")) {
      const user = this.users.get(String(this.args[0]));
      return user ? { user_id: user.userId, ik_ed25519_pub: user.publicKeyB64 } as T : null;
    }
    if (this.sql.includes("SELECT 1 ok FROM users")) {
      return this.users.has(String(this.args[0])) ? { ok: 1 } as T : null;
    }
    if (this.sql.includes("FROM mail_address_epochs WHERE user_id")) {
      const user = this.users.get(String(this.args[0]));
      return user ? activeAddressRow(user) as T : null;
    }
    if (this.sql.includes("FROM mail_address_epochs WHERE username")) {
      const user = this.byUsername.get(String(this.args[0]));
      return user ? activeAddressRow(user) as T : null;
    }
    if (this.sql.includes("FROM mail_address_epochs WHERE address")) {
      const user = this.byAddress.get(String(this.args[0]));
      return user ? activeAddressRow(user) as T : null;
    }
    if (this.sql.includes("FROM mail_sender_consents")) {
      const allowed = this.consents.get(`${this.args[0]}:${this.args[1]}`);
      return allowed === undefined ? null : { allowed } as T;
    }
    throw new Error(`unhandled first SQL: ${this.sql}`);
  }

  async run(): Promise<{ meta: { changes: number } }> {
    if (this.sql.includes("INSERT INTO mail_control_receipts")) {
      const key = `${this.args[0]}:${this.args[1]}`;
      if (this.receipts.has(key)) throw new Error("UNIQUE constraint failed: mail_control_receipts");
      this.receipts.add(key);
      return { meta: { changes: 1 } };
    }
    if (this.sql.includes("INSERT INTO mail_sender_consents")) {
      this.consents.set(`${this.args[0]}:${this.args[1]}`, Number(this.args[2]));
      return { meta: { changes: 1 } };
    }
    throw new Error(`unhandled run SQL: ${this.sql}`);
  }
}

class FakeMailbox {
  private readonly messages: MessageRecord[] = [];
  private readonly outgoing = new Map<string, string>();

  async reserveOutgoing(_ownerUserId: string, requestId: string, recipientUserId: string): Promise<{ ok: boolean; replay: boolean; reason?: string }> {
    const priorRecipient = this.outgoing.get(requestId);
    if (priorRecipient) {
      return priorRecipient === recipientUserId
        ? { ok: true, replay: true }
        : { ok: false, replay: true, reason: "request_replay_mismatch" };
    }
    this.outgoing.set(requestId, recipientUserId);
    return { ok: true, replay: false };
  }

  async store(input: {
    messageId: string;
    kind: "osl_e2ee";
    senderUserId: string | null;
    opaqueThreadToken: string;
    ciphertextB64: string;
    envelopeJson: string;
    recipientKeyFingerprint: string;
  }): Promise<{ stored: boolean; replay: boolean }> {
    if (!this.messages.some((message) => message.message_id === input.messageId)) {
      this.messages.unshift({
        message_id: input.messageId,
        kind: input.kind,
        sender_user_id: input.senderUserId,
        opaque_thread_token: input.opaqueThreadToken,
        ciphertext_b64: input.ciphertextB64,
        envelope_json: input.envelopeJson,
        recipient_key_fingerprint: input.recipientKeyFingerprint,
      });
    }
    return { stored: true, replay: false };
  }

  async list(_ownerUserId: string, limit: number): Promise<MessageRecord[]> {
    return this.messages.slice(0, limit);
  }
}

function activeAddressRow(identity: Identity): {
  address: string;
  username: string;
  user_id: string;
  address_epoch: number;
  state: "active";
} {
  return {
    address: identity.address,
    username: identity.username,
    user_id: identity.userId,
    address_epoch: 1,
    state: "active",
  };
}

function println4504(key: string, value: string | number): void {
  console.log(`TASK4504_${key}=${value}`);
}
