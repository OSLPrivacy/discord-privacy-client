import { beforeEach, describe, expect, it } from "vitest";
import { env, runInDurableObject, SELF } from "cloudflare:test";
import { base64Decode, base64Encode, mailSignedMessage, randomRequestId } from "../../src/mail/protocol.js";
import { handleInboundEmail } from "../../src/mail/inbound.js";
import type { Mailbox } from "../../src/mail/mailbox.js";

interface Identity {
  userId: string;
  username: string;
  signingKey: CryptoKey;
  x25519PrivateKey: CryptoKey;
}

beforeEach(async () => {
  await env.DB.batch([
    env.DB.prepare("DELETE FROM mail_control_receipts"),
    env.DB.prepare("DELETE FROM mail_sender_consents"),
    env.DB.prepare("DELETE FROM mail_address_epochs"),
    env.DB.prepare("DELETE FROM username_directory"),
    env.DB.prepare("DELETE FROM users"),
  ]);
});

describe("OSL Mail Worker", () => {
  it("m1 admits the OSL Mail Durable Object backend", async () => {
    const box = env.MAILBOX.getByName(`m1-${crypto.randomUUID()}`);
    const now = Date.now();
    const ciphertext = base64Encode(new TextEncoder().encode("durable ciphertext"));

    const stored = await box.store({
      ownerUserId: "owner-m1",
      messageId: "mail_m1_backend",
      requestId: "request-m1-store",
      kind: "osl_e2ee",
      senderUserId: "sender-m1",
      opaqueThreadToken: "threadtokenbackend",
      ciphertextB64: ciphertext,
      envelopeJson: JSON.stringify({ version: 1, nonce_b64: "bm9uY2U=" }),
      recipientKeyFingerprint: "fingerprint-m1",
      receivedAt: now,
      expiresAt: now + 60_000,
    });
    expect(stored).toEqual({ stored: true, replay: false });

    const listing = await box.list("owner-m1", 10);
    expect(listing).toHaveLength(1);
    expect(JSON.stringify(listing)).not.toContain(ciphertext);
    expect(listing[0]).toMatchObject({
      message_id: "mail_m1_backend",
      kind: "osl_e2ee",
      sender_user_id: "sender-m1",
      opaque_thread_token: "threadtokenbackend",
    });

    expect(await box.fetchMessage("owner-m1", "mail_m1_backend")).toMatchObject({
      message_id: "mail_m1_backend",
      ciphertext_b64: ciphertext,
      envelope_json: JSON.stringify({ version: 1, nonce_b64: "bm9uY2U=" }),
    });
    await runInDurableObject(box, async (instance: Mailbox) => {
      expect(() => instance.list("intruder-m1", 10)).toThrow("mailbox owner mismatch");
    });

    expect(await box.ack("owner-m1", "request-m1-ack", "mail_m1_backend", now)).toEqual({ deleted: true, replay: false });
    expect(await box.ack("owner-m1", "request-m1-ack", "mail_m1_backend", now)).toEqual({ deleted: true, replay: true });
    expect(await box.fetchMessage("owner-m1", "mail_m1_backend")).toBeNull();
  });

  it("advertises the truthful v1 transport and retention boundary", async () => {
    const response = await SELF.fetch("https://test/v1/mail/capabilities");
    expect(response.status).toBe(200);
    const body = await response.json() as Record<string, unknown>;
    expect(body.oslToOslE2ee).toBe(true);
    expect(body.externalInbound).toBe(true);
    expect(body.externalOutbound).toBe(false);
    expect(JSON.stringify(body)).toContain("transactional-only");
    expect(JSON.stringify(body)).toContain("72 hours");
  });

  it("m2 provisions OSL Mail addresses as immutable epochs", async () => {
    const alice = await createIdentity("alice-m2-id", "alice_m2");

    let response = await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice_m2", rotate: false });
    expect(response.status).toBe(201);
    expect(await response.json()).toMatchObject({
      address: "alice_m2@oslprivacy.com",
      address_epoch: 1,
      state: "active",
    });

    response = await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice_m2", rotate: false });
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({
      address: "alice_m2@oslprivacy.com",
      address_epoch: 1,
      replay: true,
    });
    expect(await mailEpochCount()).toBe(1);

    await replaceUsername(alice.userId, "alice_m2_next");
    response = await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice_m2_next", rotate: true });
    expect(response.status).toBe(201);
    expect(await response.json()).toMatchObject({
      address: "alice_m2_next@oslprivacy.com",
      address_epoch: 2,
      state: "active",
    });
    expect(await env.DB.prepare("SELECT state FROM mail_address_epochs WHERE address = 'alice_m2@oslprivacy.com'").first<{ state: string }>())
      .toMatchObject({ state: "tombstoned" });

    await replaceUsername(alice.userId, "alice_m2");
    response = await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice_m2", rotate: true });
    expect(response.status).toBe(409);
    expect(await env.DB.prepare("SELECT address, address_epoch, state FROM mail_address_epochs WHERE user_id = ? AND state = 'active'")
      .bind(alice.userId).first<{ address: string; address_epoch: number; state: string }>())
      .toMatchObject({ address: "alice_m2_next@oslprivacy.com", address_epoch: 2, state: "active" });
  });

  it("provisions immutable epochs and permanently tombstones a rotated address", async () => {
    const alice = await createIdentity("alice-id", "alice");
    let response = await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice", rotate: false });
    expect(response.status).toBe(201);
    expect(await response.json()).toMatchObject({ address: "alice@oslprivacy.com", address_epoch: 1, state: "active" });

    await env.DB.prepare("DELETE FROM username_directory WHERE user_id = ?").bind(alice.userId).run();
    await env.DB.prepare("INSERT INTO username_directory(username,user_id,friend_code,claimed_at,updated_at) VALUES ('alice_new',?,?,?,?)")
      .bind(alice.userId, "friend-code-placeholder", new Date().toISOString(), new Date().toISOString()).run();
    response = await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice_new", rotate: true });
    expect(response.status).toBe(201);
    expect(await response.json()).toMatchObject({ address: "alice_new@oslprivacy.com", address_epoch: 2 });
    const old = await env.DB.prepare("SELECT state FROM mail_address_epochs WHERE address='alice@oslprivacy.com'").first<{ state: string }>();
    expect(old?.state).toBe("tombstoned");

    await env.DB.prepare("DELETE FROM username_directory WHERE user_id = ?").bind(alice.userId).run();
    await env.DB.prepare("INSERT INTO username_directory(username,user_id,friend_code,claimed_at,updated_at) VALUES ('alice',?,?,?,?)")
      .bind(alice.userId, "friend-code-placeholder", new Date().toISOString(), new Date().toISOString()).run();
    response = await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice", rotate: true });
    expect(response.status).toBe(409);
    const stillActive = await env.DB.prepare("SELECT state FROM mail_address_epochs WHERE address='alice_new@oslprivacy.com'").first<{ state: string }>();
    expect(stillActive?.state).toBe("active");

    await env.DB.prepare("DELETE FROM username_directory WHERE username = 'alice'").run();
    const mallory = await createIdentity("mallory-id", "alice");
    response = await signedPost("/v1/mail/address", "PROVISION", mallory, { username: "alice", rotate: false });
    expect(response.status).toBe(409);
  });

  it("requires recipient consent, retains ciphertext only, and deletes on acknowledgement", async () => {
    const alice = await createIdentity("alice-id", "alice");
    const bob = await createIdentity("bob-id", "bob");
    expect((await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice", rotate: false })).status).toBe(201);
    expect((await signedPost("/v1/mail/address", "PROVISION", bob, { username: "bob", rotate: false })).status).toBe(201);

    const payload = {
      recipient_address: "bob@oslprivacy.com",
      opaque_thread_token: "abcdefghijklmnop",
      ciphertext_b64: base64Encode(new TextEncoder().encode("ciphertext-not-plaintext")),
      envelope: { version: 1, nonce_b64: "bm9uY2U=" },
      recipient_key_fingerprint: "fingerprint",
    };
    let response = await signedPost("/v1/mail/send/osl", "SEND-OSL", alice, payload);
    expect(response.status).toBe(403);
    expect((await signedPost("/v1/mail/consent", "CONSENT", bob, { sender_user_id: alice.userId, allowed: true })).status).toBe(200);
    response = await signedPost("/v1/mail/send/osl", "SEND-OSL", alice, payload);
    expect(response.status).toBe(200);
    const sent = await response.json() as { message_id: string };

    response = await signedPost("/v1/mail/list", "LIST", bob, { limit: 10 });
    const listed = await response.json() as { messages: Array<Record<string, unknown>> };
    expect(listed.messages).toHaveLength(1);
    expect(JSON.stringify(listed)).not.toContain("ciphertext-not-plaintext");
    expect(listed.messages[0]?.message_id).toBe(sent.message_id);

    response = await signedPost("/v1/mail/fetch", "FETCH", bob, { message_id: sent.message_id });
    expect(await response.json()).toMatchObject({ ciphertext_b64: payload.ciphertext_b64, kind: "osl_e2ee" });
    response = await signedPost("/v1/mail/ack", "ACK", bob, { message_id: sent.message_id });
    expect(await response.json()).toMatchObject({ deleted: true, replay: false });
    response = await signedPost("/v1/mail/fetch", "FETCH", bob, { message_id: sent.message_id });
    expect(response.status).toBe(404);
  });

  it("m3 sends OSL-to-OSL Mail with recipient consent", async () => {
    const alice = await createIdentity("alice-m3-id", "alice_m3");
    const bob = await createIdentity("bob-m3-id", "bob_m3");
    const carol = await createIdentity("carol-m3-id", "carol_m3");
    expect((await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice_m3", rotate: false })).status).toBe(201);
    expect((await signedPost("/v1/mail/address", "PROVISION", bob, { username: "bob_m3", rotate: false })).status).toBe(201);
    expect((await signedPost("/v1/mail/address", "PROVISION", carol, { username: "carol_m3", rotate: false })).status).toBe(201);

    const carolPayload = oslPayload("bob_m3@oslprivacy.com", "carol ciphertext");
    expect((await signedPost("/v1/mail/send/osl", "SEND-OSL", carol, carolPayload)).status).toBe(403);
    expect((await signedPost("/v1/mail/list", "LIST", bob, { limit: 10 }).then((response) => response.json()) as { messages: unknown[] }).messages)
      .toHaveLength(0);

    expect((await signedPost("/v1/mail/consent", "CONSENT", bob, { sender_user_id: alice.userId, allowed: true })).status).toBe(200);
    const alicePayload = oslPayload("bob_m3@oslprivacy.com", "alice ciphertext");
    const delivered = await signedPost("/v1/mail/send/osl", "SEND-OSL", alice, alicePayload);
    expect(delivered.status).toBe(200);
    const delivery = await delivered.json() as { message_id: string; accepted: boolean; replay: boolean };
    expect(delivery).toMatchObject({ accepted: true, replay: false });

    const listed = await signedPost("/v1/mail/list", "LIST", bob, { limit: 10 }).then((response) => response.json()) as {
      messages: Array<{ message_id: string; sender_user_id: string; kind: string }>;
    };
    expect(listed.messages).toHaveLength(1);
    expect(listed.messages[0]).toMatchObject({
      message_id: delivery.message_id,
      sender_user_id: alice.userId,
      kind: "osl_e2ee",
    });
    const fetched = await signedPost("/v1/mail/fetch", "FETCH", bob, { message_id: delivery.message_id });
    expect(await fetched.json()).toMatchObject({
      ciphertext_b64: alicePayload.ciphertext_b64,
      recipient_key_fingerprint: alicePayload.recipient_key_fingerprint,
    });
  });

  it("qualifies the OSL-to-OSL lifecycle from provisioning through burn", async () => {
    const alice = await createIdentity("alice-life-id", "alice_life");
    const bob = await createIdentity("bob-life-id", "bob_life");
    expect((await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice_life", rotate: false })).status).toBe(201);
    expect((await signedPost("/v1/mail/address", "PROVISION", bob, { username: "bob_life", rotate: false })).status).toBe(201);
    expect((await signedPost("/v1/mail/consent", "CONSENT", bob, { sender_user_id: alice.userId, allowed: true })).status).toBe(200);

    const sendRequestId = randomRequestId();
    const payload = {
      request_id: sendRequestId,
      recipient_address: "bob_life@oslprivacy.com",
      opaque_thread_token: "threadtokenlifecycle",
      ciphertext_b64: base64Encode(new TextEncoder().encode("sealed lifecycle payload")),
      envelope: { version: 1, nonce_b64: "bm9uY2U=" },
      recipient_key_fingerprint: "fingerprint",
    };
    let response = await signedPost("/v1/mail/send/osl", "SEND-OSL", alice, payload);
    expect(response.status).toBe(200);
    const sent = await response.json() as { message_id: string; accepted: boolean; replay: boolean };
    expect(sent.accepted).toBe(true);
    expect(sent.replay).toBe(false);

    response = await signedPost("/v1/mail/send/osl", "SEND-OSL", alice, payload);
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({ message_id: sent.message_id, accepted: true, replay: true });

    response = await signedPost("/v1/mail/list", "LIST", bob, { limit: 10 });
    const listText = await response.text();
    expect(listText).not.toContain("sealed lifecycle payload");
    const listed = JSON.parse(listText) as { messages: Array<{ message_id: string; kind: string }> };
    expect(listed.messages).toHaveLength(1);
    expect(listed.messages[0]).toMatchObject({ message_id: sent.message_id, kind: "osl_e2ee" });

    response = await signedPost("/v1/mail/fetch", "FETCH", bob, { message_id: sent.message_id });
    expect(await response.json()).toMatchObject({
      message_id: sent.message_id,
      ciphertext_b64: payload.ciphertext_b64,
      sender_user_id: alice.userId,
      opaque_thread_token: payload.opaque_thread_token,
    });

    response = await signedPost("/v1/mail/ack", "ACK", bob, { message_id: sent.message_id });
    expect(await response.json()).toMatchObject({ deleted: true, replay: false });
    response = await signedPost("/v1/mail/list", "LIST", bob, { limit: 10 });
    expect((await response.json() as { messages: unknown[] }).messages).toHaveLength(0);

    response = await signedPost("/v1/mail/burn", "BURN", bob, {});
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({ deleted: 0, address_tombstoned: true, replay: false });
    response = await signedPost("/v1/mail/list", "LIST", bob, { limit: 10 });
    expect(response.status).toBe(404);
    const row = await env.DB.prepare("SELECT state FROM mail_address_epochs WHERE address='bob_life@oslprivacy.com'").first<{ state: string }>();
    expect(row?.state).toBe("tombstoned");
  });

  it("refuses OSL-to-OSL delivery after recipient revokes sender consent", async () => {
    const alice = await createIdentity("alice-id", "alice");
    const bob = await createIdentity("bob-id", "bob");
    expect((await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice", rotate: false })).status).toBe(201);
    expect((await signedPost("/v1/mail/address", "PROVISION", bob, { username: "bob", rotate: false })).status).toBe(201);
    expect((await signedPost("/v1/mail/consent", "CONSENT", bob, { sender_user_id: alice.userId, allowed: true })).status).toBe(200);
    expect((await signedPost("/v1/mail/consent", "CONSENT", bob, { sender_user_id: alice.userId, allowed: false })).status).toBe(200);

    const response = await signedPost("/v1/mail/send/osl", "SEND-OSL", alice, {
      recipient_address: "bob@oslprivacy.com",
      opaque_thread_token: "abcdefghijklmnop",
      ciphertext_b64: base64Encode(new TextEncoder().encode("ciphertext-not-plaintext")),
      envelope: { version: 1, nonce_b64: "bm9uY2U=" },
      recipient_key_fingerprint: "fingerprint",
    });
    expect(response.status).toBe(403);

    const list = await signedPost("/v1/mail/list", "LIST", bob, { limit: 10 });
    const body = await list.json() as { messages: unknown[] };
    expect(body.messages).toHaveLength(0);
  });

  it("refuses OSL-to-OSL delivery from a sender without an active mailbox even with consent", async () => {
    const alice = await createIdentity("alice-id", "alice");
    const bob = await createIdentity("bob-id", "bob");
    expect((await signedPost("/v1/mail/address", "PROVISION", bob, { username: "bob", rotate: false })).status).toBe(201);
    expect((await signedPost("/v1/mail/consent", "CONSENT", bob, { sender_user_id: alice.userId, allowed: true })).status).toBe(200);

    const response = await signedPost("/v1/mail/send/osl", "SEND-OSL", alice, {
      recipient_address: "bob@oslprivacy.com",
      opaque_thread_token: "abcdefghijklmnop",
      ciphertext_b64: base64Encode(new TextEncoder().encode("ciphertext-not-plaintext")),
      envelope: { version: 1, nonce_b64: "bm9uY2U=" },
      recipient_key_fingerprint: "fingerprint",
    });
    expect(response.status).toBe(403);

    const list = await signedPost("/v1/mail/list", "LIST", bob, { limit: 10 });
    const body = await list.json() as { messages: unknown[] };
    expect(body.messages).toHaveLength(0);
  });

  it("burns the mailbox atomically and tombstones its address", async () => {
    const alice = await createIdentity("alice-id", "alice");
    await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice", rotate: false });
    const response = await signedPost("/v1/mail/burn", "BURN", alice, {});
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({ deleted: 0, address_tombstoned: true, replay: false });
    const row = await env.DB.prepare("SELECT state FROM mail_address_epochs WHERE address='alice@oslprivacy.com'").first<{ state: string }>();
    expect(row?.state).toBe("tombstoned");
  });

  it("fails external outbound honestly without invoking a send binding", async () => {
    const response = await SELF.fetch("https://test/v1/mail/send/external", { method: "POST", body: "{}", headers: { "content-type": "application/json" } });
    expect(response.status).toBe(503);
    expect(await response.text()).toContain("transactional-only");
  });

  it("rejects catch-all mail for unprovisioned recipients before reading MIME", async () => {
    let rejected = "";
    let rawRead = false;
    const message = {
      from: "sender@example.com",
      to: "nobody@oslprivacy.com",
      rawSize: 50,
      headers: new Headers(),
      get raw() {
        rawRead = true;
        return new ReadableStream<Uint8Array>();
      },
      setReject(reason: string) { rejected = reason; },
    } as unknown as ForwardableEmailMessage;
    await handleInboundEmail(message, env);
    expect(rejected).toContain("not provisioned");
    expect(rawRead).toBe(false);
  });

  it("m4 envelope-encrypts external inbound mail without retaining plaintext", async () => {
    const bob = await createIdentity("bob-id", "bob");
    await signedPost("/v1/mail/address", "PROVISION", bob, { username: "bob", rotate: false });
    const mime = "From: outside@example.com\r\nSubject: private subject\r\nMessage-ID: <thread@example.com>\r\n\r\nsecret external body";
    const rawChunk = new TextEncoder().encode(mime);
    let rejected = "";
    const message = {
      from: "outside@example.com",
      to: "bob@oslprivacy.com",
      rawSize: rawChunk.byteLength,
      headers: new Headers({ "message-id": "<thread@example.com>" }),
      raw: new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(rawChunk);
          controller.close();
        },
      }),
      setReject(reason: string) { rejected = reason; },
    } as unknown as ForwardableEmailMessage;
    await handleInboundEmail(message, env);
    expect(rejected).toBe("");
    expect(Array.from(rawChunk)).toEqual(Array(rawChunk.byteLength).fill(0));
    const response = await signedPost("/v1/mail/list", "LIST", bob, { limit: 10 });
    const listingText = await response.text();
    expect(listingText).not.toContain("private subject");
    expect(listingText).not.toContain("outside@example.com");
    const listing = JSON.parse(listingText) as { messages: Array<{ message_id: string; kind: string; expires_at: number; received_at: number }> };
    expect(listing.messages[0]?.kind).toBe("external_envelope");
    expect((listing.messages[0]?.expires_at ?? 0) - (listing.messages[0]?.received_at ?? 0)).toBe(72 * 60 * 60 * 1000);
    const fetched = await signedPost("/v1/mail/fetch", "FETCH", bob, { message_id: listing.messages[0]!.message_id });
    const fetchedText = await fetched.text();
    expect(fetchedText).not.toContain("private subject");
    expect(fetchedText).not.toContain("outside@example.com");
    expect(fetchedText).not.toContain("secret external body");
    const fetchedBody = JSON.parse(fetchedText) as {
      ciphertext_b64: string;
      envelope_json: string;
      kind: string;
    };
    expect(fetchedBody.kind).toBe("external_envelope");
    const envelope = JSON.parse(fetchedBody.envelope_json) as {
      algorithm: string;
      ephemeral_public_key_b64: string;
      salt_b64: string;
      nonce_b64: string;
      aad_b64: string;
    };
    expect(envelope.algorithm).toBe("X25519-HKDF-SHA256-AES-256-GCM");
    const ciphertext = base64Decode(fetchedBody.ciphertext_b64);
    expect(new TextDecoder().decode(ciphertext)).not.toContain("secret external body");
    const ephemeral = await crypto.subtle.importKey("raw", base64Decode(envelope.ephemeral_public_key_b64), { name: "X25519" }, false, []);
    const shared = await crypto.subtle.deriveBits(
      { name: "X25519", public: ephemeral } as unknown as SubtleCryptoDeriveKeyAlgorithm,
      bob.x25519PrivateKey,
      256,
    );
    const hkdf = await crypto.subtle.importKey("raw", shared, "HKDF", false, ["deriveKey"]);
    const aes = await crypto.subtle.deriveKey(
      { name: "HKDF", hash: "SHA-256", salt: base64Decode(envelope.salt_b64), info: new TextEncoder().encode("OSL external inbound v1") },
      hkdf,
      { name: "AES-GCM", length: 256 },
      false,
      ["decrypt"],
    );
    const plaintext = await crypto.subtle.decrypt(
      {
        name: "AES-GCM",
        iv: base64Decode(envelope.nonce_b64),
        additionalData: base64Decode(envelope.aad_b64),
        tagLength: 128,
      },
      aes,
      ciphertext,
    );
    expect(new TextDecoder().decode(plaintext)).toBe(mime);
  });

  it("rejects every non-OSL recipient on the internal E2EE route", async () => {
    const alice = await createIdentity("alice-id", "alice");
    await signedPost("/v1/mail/address", "PROVISION", alice, { username: "alice", rotate: false });
    const response = await signedPost("/v1/mail/send/osl", "SEND-OSL", alice, {
      recipient_address: "person@example.com",
      opaque_thread_token: "abcdefghijklmnop",
      ciphertext_b64: base64Encode(new Uint8Array([1, 2, 3])),
      envelope: { version: 1 },
      recipient_key_fingerprint: "fingerprint",
    });
    expect(response.status).toBe(404);
  });
});

async function createIdentity(userId: string, username: string): Promise<Identity> {
  const ed = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]) as CryptoKeyPair;
  const edRaw = await crypto.subtle.exportKey("raw", ed.publicKey) as ArrayBuffer;
  const x = await crypto.subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]) as CryptoKeyPair;
  const xRaw = await crypto.subtle.exportKey("raw", x.publicKey) as ArrayBuffer;
  const now = new Date().toISOString();
  await env.DB.prepare(
    `INSERT INTO users(user_id,ik_x25519_pub,ik_ed25519_pub,ik_mlkem768_pub,ik_x25519_signature,registered_at,ik_ratchet_initial_pub)
     VALUES (?,?,?,?,?,?,?)`,
  ).bind(userId, base64Encode(new Uint8Array(xRaw)), base64Encode(new Uint8Array(edRaw)), "mlkem", "signature", now, null).run();
  await env.DB.prepare(
    "INSERT INTO username_directory(username,user_id,friend_code,claimed_at,updated_at) VALUES (?,?,?,?,?)",
  ).bind(username, userId, "friend-code-placeholder", now, now).run();
  return { userId, username, signingKey: ed.privateKey, x25519PrivateKey: x.privateKey };
}

async function replaceUsername(userId: string, username: string): Promise<void> {
  const now = new Date().toISOString();
  await env.DB.prepare("DELETE FROM username_directory WHERE user_id = ?").bind(userId).run();
  await env.DB.prepare(
    "INSERT INTO username_directory(username,user_id,friend_code,claimed_at,updated_at) VALUES (?,?,?,?,?)",
  ).bind(username, userId, "friend-code-placeholder", now, now).run();
}

async function mailEpochCount(): Promise<number> {
  const row = await env.DB.prepare("SELECT COUNT(*) count FROM mail_address_epochs").first<{ count: number }>();
  return row?.count ?? 0;
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

async function signedPost(
  path: string,
  operation: string,
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
  return await SELF.fetch(`https://test${path}`, {
    method: "POST",
    headers: { "content-type": "application/json", "x-forwarded-for": "192.0.2.200" },
    body: JSON.stringify(body),
  });
}
