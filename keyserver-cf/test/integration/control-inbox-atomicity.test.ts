import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { canonicalControlInboxPostBytes } from "../../src/lib/canonical.js";
import { base64Encode, registerTestUser, signEd25519 } from "./helpers.js";

let seq = 0;
const userId = (prefix: string) => `${prefix}-${Date.now().toString(36)}-${seq++}`;

const SEVEN_DAYS = 7 * 24 * 60 * 60;
const ORDINARY_RECIPIENT_CAP = 512;
const ORDINARY_PAIR_CAP = 32;

async function signedPostBody(
  senderId: string,
  recipientId: string,
  signingKey: CryptoKey,
): Promise<Record<string, unknown>> {
  const timestampMs = Date.now();
  const bundle = new TextEncoder().encode(`bundle-${seq++}`);
  const bundleHash = new Uint8Array(await crypto.subtle.digest("SHA-256", bundle));
  const scopeId = `scope-${seq++}`;
  return {
    sender_id: senderId,
    recipient_id: recipientId,
    scope_id: scopeId,
    timestamp_ms: timestampMs,
    bundle_b64: base64Encode(bundle),
    signature_b64: await signEd25519(
      signingKey,
      canonicalControlInboxPostBytes({
        sender_id: senderId,
        recipient_id: recipientId,
        scope_id: scopeId,
        timestamp_ms: timestampMs,
        bundle_sha256: bundleHash,
      }),
    ),
  };
}

async function post(body: Record<string, unknown>): Promise<Response> {
  return SELF.fetch("http://test/v1/control-inbox", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function fillOrdinaryLane(
  recipientId: string,
  rows: number,
  createdAt: number,
): Promise<void> {
  const statements = [];
  for (let index = 0; index < rows; index++) {
    const id = new Uint8Array(16);
    crypto.getRandomValues(id);
    statements.push(
      env.DB.prepare(
        `INSERT INTO control_inbox
           (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at, kind, collapse_key)
         VALUES (?, ?, ?, ?, ?, ?, ?, '', NULL)`,
      ).bind(
        id,
        recipientId,
        `filler-${recipientId}-${Math.floor(index / ORDINARY_PAIR_CAP)}`,
        `filler-scope-${index}`,
        new Uint8Array([index & 0xff]),
        Math.floor(Date.now() / 1000) + SEVEN_DAYS,
        createdAt + index,
      ),
    );
  }
  await env.DB.batch(statements);
}

async function liveOrdinaryRowCount(recipientId: string): Promise<number> {
  const row = await env.DB.prepare(
    `SELECT COUNT(*) AS count FROM control_inbox
      WHERE recipient_id = ? AND kind = '' AND expires_at >= ?`,
  )
    .bind(recipientId, Math.floor(Date.now() / 1000))
    .first<{ count: number }>();
  return row?.count ?? 0;
}

async function inboxRowCount(senderId: string, recipientId: string): Promise<number> {
  const row = await env.DB.prepare(
    `SELECT COUNT(*) AS count FROM control_inbox
      WHERE sender_id = ? AND recipient_id = ?`,
  )
    .bind(senderId, recipientId)
    .first<{ count: number }>();
  return row?.count ?? 0;
}

async function requestReceiptCount(senderId: string, recipientId: string): Promise<number> {
  const row = await env.DB.prepare(
    `SELECT COUNT(*) AS count FROM control_inbox_requests
      WHERE sender_id = ? AND recipient_id = ?`,
  )
    .bind(senderId, recipientId)
    .first<{ count: number }>();
  return row?.count ?? 0;
}

async function inboxRowsForSender(senderId: string): Promise<number> {
  const row = await env.DB.prepare(
    "SELECT COUNT(*) AS count FROM control_inbox WHERE sender_id = ?",
  )
    .bind(senderId)
    .first<{ count: number }>();
  return row?.count ?? 0;
}

async function requestReceiptsForSender(senderId: string): Promise<number> {
  const row = await env.DB.prepare(
    "SELECT COUNT(*) AS count FROM control_inbox_requests WHERE sender_id = ?",
  )
    .bind(senderId)
    .first<{ count: number }>();
  return row?.count ?? 0;
}

describe("control inbox atomic admission", () => {
  it("keeps the recipient-wide ordinary cap under concurrent posts", async () => {
    const recipientId = userId("recipient");
    const senderAId = userId("sender-a");
    const senderBId = userId("sender-b");
    await registerTestUser(SELF, recipientId);
    const senderA = await registerTestUser(SELF, senderAId);
    const senderB = await registerTestUser(SELF, senderBId);

    const now = Math.floor(Date.now() / 1000);
    await fillOrdinaryLane(recipientId, ORDINARY_RECIPIENT_CAP - 1, now);
    expect(await liveOrdinaryRowCount(recipientId)).toBe(ORDINARY_RECIPIENT_CAP - 1);

    // Sign before Promise.all so request concurrency is not hidden behind
    // WebCrypto work in the test harness.
    const bodyA = await signedPostBody(senderAId, recipientId, senderA.signingKey);
    const bodyB = await signedPostBody(senderBId, recipientId, senderB.signingKey);
    const responses = await Promise.all([post(bodyA), post(bodyB)]);
    const statuses = responses.map((response) => response.status).sort();

    expect(statuses).toEqual([201, 429]);
    const full = responses.find((response) => response.status === 429);
    expect(full, "one concurrent post must be refused").toBeDefined();
    expect(await full!.json()).toMatchObject({
      error: "recipient_inbox_full",
      scope: "recipient",
    });
    expect(await liveOrdinaryRowCount(recipientId)).toBe(ORDINARY_RECIPIENT_CAP);
  });

  it("leaves no inbox row or request receipt when the recipient does not exist", async () => {
    const controlSenderId = userId("control-sender");
    const controlRecipientId = userId("control-recipient");
    const missingSenderId = userId("missing-sender");
    const missingRecipientId = userId("missing-recipient");
    const controlSender = await registerTestUser(SELF, controlSenderId);
    await registerTestUser(SELF, controlRecipientId);
    const missingSender = await registerTestUser(SELF, missingSenderId);

    const control = await post(
      await signedPostBody(controlSenderId, controlRecipientId, controlSender.signingKey),
    );
    expect(control.status).toBe(201);
    expect(await inboxRowCount(controlSenderId, controlRecipientId)).toBe(1);
    expect(await requestReceiptCount(controlSenderId, controlRecipientId)).toBe(1);

    const failed = await post(
      await signedPostBody(missingSenderId, missingRecipientId, missingSender.signingKey),
    );
    expect(failed.status).toBe(404);
    expect(await inboxRowsForSender(missingSenderId)).toBe(0);
    expect(await requestReceiptsForSender(missingSenderId)).toBe(0);
  });

  it("writes both the inbox row and request receipt on a successful post", async () => {
    const senderId = userId("sender");
    const recipientId = userId("recipient");
    const sender = await registerTestUser(SELF, senderId);
    await registerTestUser(SELF, recipientId);

    const response = await post(await signedPostBody(senderId, recipientId, sender.signingKey));
    expect(response.status).toBe(201);
    expect(await inboxRowCount(senderId, recipientId)).toBe(1);
    expect(await requestReceiptCount(senderId, recipientId)).toBe(1);
  });
});
