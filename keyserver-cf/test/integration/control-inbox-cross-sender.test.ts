/// MEDIUM regression suite — "any registered sender can evict another
/// sender's undelivered control messages"
/// (docs/security/osl-audit-2026-07-26-codex.md).
///
/// The ordinary lane used to answer a full recipient inbox by deleting the
/// oldest undelivered rows *for that recipient*, regardless of who sent them.
/// Registration is open, so an attacker with a handful of identities could
/// silently destroy an unrelated sender's pending SKDM/control state and the
/// victim learned nothing.

import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { canonicalControlInboxPostBytes } from "../../src/lib/canonical.js";
import { base64Encode, registerTestUser, signEd25519 } from "./helpers.js";

let seq = 0;
const userId = (prefix: string) => `${prefix}-${Date.now().toString(36)}-${seq++}`;

const SEVEN_DAYS = 7 * 24 * 60 * 60;

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

/// Fill a recipient's ordinary lane directly. Going through the endpoint for
/// hundreds of rows would spend the whole test budget on Ed25519 signatures
/// without exercising anything the endpoint tests do not already cover; what
/// matters here is the state the admission logic then sees. Rows are spread
/// across distinct sender ids so the per-pair cap is respected, exactly as it
/// would be for an attacker holding several open registrations.
async function fillOrdinaryLane(
  recipientId: string,
  rows: number,
  createdAt: number,
): Promise<void> {
  const perSender = 32;
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
        `filler-${Math.floor(index / perSender)}`,
        `filler-scope-${index}`,
        new Uint8Array([index & 0xff]),
        Math.floor(Date.now() / 1000) + SEVEN_DAYS,
        createdAt + index,
      ),
    );
  }
  await env.DB.batch(statements);
}

async function liveRowCount(recipientId: string): Promise<number> {
  const row = await env.DB.prepare(
    `SELECT COUNT(*) AS count FROM control_inbox
      WHERE recipient_id = ? AND kind = '' AND expires_at >= ?`,
  )
    .bind(recipientId, Math.floor(Date.now() / 1000))
    .first<{ count: number }>();
  return row?.count ?? 0;
}

describe("control inbox admission never destroys an unrelated sender's rows", () => {
  it("keeps a victim's queued row when an attacker fills the recipient's lane", async () => {
    const recipientId = userId("recipient");
    const victimId = userId("victim");
    const attackerId = userId("attacker");
    await registerTestUser(SELF, recipientId);
    const victim = await registerTestUser(SELF, victimId);
    const attacker = await registerTestUser(SELF, attackerId);

    // The victim's single legitimate, undelivered control message. It is the
    // oldest row in the lane, so it is the first eviction victim.
    const victimPost = await post(await signedPostBody(victimId, recipientId, victim.signingKey));
    expect(victimPost.status).toBe(201);
    const victimRow = (await victimPost.json()) as { id: string };

    const now = Math.floor(Date.now() / 1000);
    await fillOrdinaryLane(recipientId, 511, now + 10);
    expect(await liveRowCount(recipientId)).toBe(512);

    // The attacker posts one more. Before the fix this silently deletes the
    // victim's row to make space and answers 201.
    const attackerPost = await post(
      await signedPostBody(attackerId, recipientId, attacker.signingKey),
    );

    const survivor = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM control_inbox WHERE recipient_id = ? AND sender_id = ?",
    )
      .bind(recipientId, victimId)
      .first<{ count: number }>();
    expect(survivor?.count, `victim row ${victimRow.id} was evicted`).toBe(1);

    // ...and the attacker is told, rather than being quietly served at the
    // victim's expense.
    expect(attackerPost.status).toBe(429);
    expect(await attackerPost.json()).toMatchObject({
      error: "recipient_inbox_full",
      scope: "recipient",
    });
  });

  it("still lets an unrelated sender reach a recipient whose lane is congested", async () => {
    const recipientId = userId("recipient");
    const newcomerId = userId("newcomer");
    await registerTestUser(SELF, recipientId);
    const newcomer = await registerTestUser(SELF, newcomerId);

    const now = Math.floor(Date.now() / 1000);
    await fillOrdinaryLane(recipientId, 400, now);

    // Reserved headroom exists precisely so congestion caused by other senders
    // cannot make a first contact undeliverable.
    const response = await post(
      await signedPostBody(newcomerId, recipientId, newcomer.signingKey),
    );
    expect(response.status).toBe(201);
  });

  it("destroys nothing when it refuses, not even the sender's own rows", async () => {
    // Regression test for a defect introduced by the cross-sender fix itself
    // and caught in adversarial review. Eviction used to run BEFORE the
    // recipient-wide check, so a sender at its pair cap posting to a congested
    // recipient had one of its OWN queued rows deleted to make room and was
    // then refused anyway: undelivered control state destroyed on a path that
    // reports failure. A refusal must be inert.
    const recipientId = userId("recipient");
    const senderId = userId("capped");
    await registerTestUser(SELF, recipientId);
    const sender = await registerTestUser(SELF, senderId);

    const now = Math.floor(Date.now() / 1000);
    // 32 rows from this sender — exactly the per-pair cap — plus 480 from
    // others, putting the recipient at the 512 recipient-wide cap.
    const own = [];
    for (let index = 0; index < 32; index++) {
      const id = new Uint8Array(16);
      crypto.getRandomValues(id);
      own.push(
        env.DB.prepare(
          `INSERT INTO control_inbox
             (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at, kind, collapse_key)
           VALUES (?, ?, ?, ?, ?, ?, ?, '', NULL)`,
        ).bind(
          id,
          recipientId,
          senderId,
          `own-scope-${index}`,
          new Uint8Array([1]),
          now + SEVEN_DAYS,
          now + index,
        ),
      );
    }
    await env.DB.batch(own);
    await fillOrdinaryLane(recipientId, 480, now + 100);
    expect(await liveRowCount(recipientId)).toBe(512);

    const response = await post(await signedPostBody(senderId, recipientId, sender.signingKey));
    expect(response.status).toBe(429);

    // The whole point: the sender still holds all 32. A refusal that quietly
    // costs the sender a queued message is not a refusal, it is a deletion.
    const held = await env.DB.prepare(
      `SELECT COUNT(*) AS count FROM control_inbox
        WHERE recipient_id = ? AND sender_id = ? AND kind = ''`,
    )
      .bind(recipientId, senderId)
      .first<{ count: number }>();
    expect(held?.count).toBe(32);
    // ...and the lane as a whole is untouched.
    expect(await liveRowCount(recipientId)).toBe(512);
  });

  it("recycles a sender's own oldest rows rather than refusing them", async () => {
    const recipientId = userId("recipient");
    const senderId = userId("chatty");
    await registerTestUser(SELF, recipientId);
    const sender = await registerTestUser(SELF, senderId);

    // 32 is the per-pair cap; the 33rd must still be accepted, displacing only
    // this sender's own stalest row.
    for (let index = 0; index < 33; index++) {
      const response = await post(await signedPostBody(senderId, recipientId, sender.signingKey));
      expect(response.status, `post ${index} was refused`).toBe(201);
    }

    const held = await env.DB.prepare(
      `SELECT COUNT(*) AS count FROM control_inbox
        WHERE recipient_id = ? AND sender_id = ? AND kind = ''`,
    )
      .bind(recipientId, senderId)
      .first<{ count: number }>();
    expect(held?.count).toBe(32);
  }, 20_000);
});
