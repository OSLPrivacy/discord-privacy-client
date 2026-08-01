// T6-K5 — "don't silently lose messages."
//
// `fetchWrappedKeyAuthenticated()` used to issue the read and the DELETE in a
// single `db.batch()`, so a `single_use` wrapped key was destroyed the moment
// it was handed to the socket. A lost HTTP response therefore destroyed the
// only copy of a message: the recipient saw nothing, and there was no
// recovery path. That violates owner decision D15 — delete on ACKNOWLEDGED
// receipt, never on transmission.
//
// These tests prove the authenticated GET is now idempotent, and that
// "retained" still means "retained until a bounded TTL", not "retained
// forever". Every assertion reads the real `wrapped_keys` rows out of the
// D1 database the Worker just wrote to; none of them inspect source text.
//
// SCOPE — what is NOT proven here, on purpose: single-use delivery WITH
// recovery (a reservation window plus a recipient-authenticated ACK that
// authorizes the delete, 03-CONTRACTS/storage.md §3). The baseline schema has
// no `reserved_until` and no `acked_at` column, there is no ACK route, and
// there is no Rust ACK client. Those are deferred; this file proves only that
// nothing is destroyed by transmission.
import { SELF, env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import {
  canonicalBurnBytes,
  canonicalWrappedKeyGetBytes,
  canonicalWrappedKeyPostBytes,
} from "../../src/lib/canonical.js";
import { sweepExpiredPrivacyRows } from "../../src/lib/db.js";
import { MAX_WRAPPED_KEY_LIFETIME_MS } from "../../src/endpoints/wrapped-keys.js";
import { base64Encode, registerTestUser, signEd25519 } from "./helpers.js";

const testDb = (env as unknown as { DB: D1Database }).DB;

interface StoredWrappedKey {
  content_id: string;
  wrapped_share_blob: string;
  single_use: number;
  expires_at: string;
}

/** Read the row the Worker actually persisted, not whatever it returned. */
async function storedRow(contentId: string): Promise<StoredWrappedKey | null> {
  return await testDb
    .prepare(
      `SELECT content_id, wrapped_share_blob, single_use, expires_at
         FROM wrapped_keys WHERE content_id = ?`,
    )
    .bind(contentId)
    .first<StoredWrappedKey>();
}

describe("T6-K5 authenticated wrapped-key GET is not destructive", () => {
  let senderId: string;
  let senderSigningKey: CryptoKey;
  let recipientId: string;
  let recipientSigningKey: CryptoKey;
  let getSequence = 0;

  beforeEach(async () => {
    senderId = `k5-sender-${Math.random().toString(36).slice(2, 8)}`;
    senderSigningKey = (await registerTestUser(SELF, senderId)).signingKey;
    recipientId = `k5-recipient-${Math.random().toString(36).slice(2, 8)}`;
    recipientSigningKey = (await registerTestUser(SELF, recipientId)).signingKey;
  });

  async function seedSingleUseKey(
    overrides: Partial<Record<string, unknown>> = {},
  ): Promise<{ contentId: string; blob: string }> {
    const body: Record<string, unknown> = {
      content_id: `k5-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
      content_type: "text",
      system_message_kind: null,
      sender_id: senderId,
      recipient_id: recipientId,
      session_version: 1,
      share_index: 0,
      wrapped_share_blob: base64Encode(crypto.getRandomValues(new Uint8Array(32))),
      blob_version: 1,
      single_use: true,
      display_duration_seconds: 10,
      expires_at: new Date(Date.now() + 5 * 60_000).toISOString(),
      timestamp_ms: Date.now(),
      ...overrides,
    };
    const message = canonicalWrappedKeyPostBytes({
      content_id: body.content_id as string,
      content_type: body.content_type as string,
      system_message_kind: body.system_message_kind as string | null,
      sender_id: body.sender_id as string,
      recipient_id: body.recipient_id as string,
      session_version: body.session_version as number,
      share_index: body.share_index as number,
      wrapped_share_blob: body.wrapped_share_blob as string,
      blob_version: body.blob_version as number,
      single_use: body.single_use as boolean,
      display_duration_seconds: body.display_duration_seconds as number | null,
      expires_at: body.expires_at as string,
      timestamp_ms: body.timestamp_ms as number,
    });
    const response = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        ...body,
        sender_signature_b64: await signEd25519(senderSigningKey, message),
      }),
    });
    expect(response.status).toBe(201);
    return {
      contentId: body.content_id as string,
      blob: body.wrapped_share_blob as string,
    };
  }

  /** Each call produces a distinct signature, as a real retrying client would. */
  async function signedGetUrl(contentId: string): Promise<string> {
    const timestampMs = Date.now() + getSequence++;
    const message = canonicalWrappedKeyGetBytes({
      requester_id: recipientId,
      recipient_id: recipientId,
      content_id: contentId,
      timestamp_ms: timestampMs,
    });
    const q = new URLSearchParams({
      requester_id: recipientId,
      recipient_id: recipientId,
      ts: String(timestampMs),
      sig: await signEd25519(recipientSigningKey, message),
    });
    return `http://test/v1/wrapped-keys/${encodeURIComponent(contentId)}?${q}`;
  }

  it("survives a lost response: the key is still in D1 and still fetchable", async () => {
    const { contentId, blob } = await seedSingleUseKey();
    expect((await storedRow(contentId))?.wrapped_share_blob).toBe(blob);

    // Attempt one. The server answers 200 — and then the response is LOST:
    // we cancel the body without ever reading it, which is exactly what a
    // dropped connection looks like to the recipient. The recipient has no
    // key material at this point.
    const attempt1 = await SELF.fetch(await signedGetUrl(contentId));
    expect(attempt1.status).toBe(200);
    await attempt1.body?.cancel();

    // The message must still exist. This is the assertion the old
    // read+DELETE batch could not satisfy.
    const afterLostResponse = await storedRow(contentId);
    expect(afterLostResponse).not.toBeNull();
    expect(afterLostResponse?.wrapped_share_blob).toBe(blob);
    expect(afterLostResponse?.single_use).toBe(1);

    // Attempt two — the retry a real client makes, with a fresh signature.
    // It must return the same key material.
    const attempt2 = await SELF.fetch(await signedGetUrl(contentId));
    expect(attempt2.status).toBe(200);
    const delivered = (await attempt2.json()) as Record<string, unknown>;
    expect(delivered.content_id).toBe(contentId);
    expect(delivered.wrapped_share_blob).toBe(blob);
    expect(delivered.single_use).toBe(true);

    // And reading it twice still did not destroy it.
    expect((await storedRow(contentId))?.wrapped_share_blob).toBe(blob);
  });

  it("stays idempotent across repeated fetches", async () => {
    const { contentId, blob } = await seedSingleUseKey();
    for (let i = 0; i < 3; i++) {
      const res = await SELF.fetch(await signedGetUrl(contentId));
      expect(res.status).toBe(200);
      const j = (await res.json()) as Record<string, unknown>;
      expect(j.wrapped_share_blob).toBe(blob);
    }
    expect((await storedRow(contentId))?.wrapped_share_blob).toBe(blob);
  });

  it("retains only until a bounded TTL, which the sweep enforces", async () => {
    // Bounded: the POST route refuses a lifetime beyond 7 days, so "retain
    // until TTL" cannot become "retain forever".
    expect(MAX_WRAPPED_KEY_LIFETIME_MS).toBe(7 * 24 * 60 * 60 * 1000);
    const tooLong = await seedSingleUseKey({
      expires_at: new Date(
        Date.now() + MAX_WRAPPED_KEY_LIFETIME_MS + 60_000,
      ).toISOString(),
    }).then(
      () => "accepted",
      () => "refused",
    );
    expect(tooLong).toBe("refused");

    // Swept: once past expires_at, the retention sweep physically removes it
    // even though no one ever fetched it.
    const { contentId } = await seedSingleUseKey();
    expect(await storedRow(contentId)).not.toBeNull();
    await testDb
      .prepare("UPDATE wrapped_keys SET expires_at = ? WHERE content_id = ?")
      .bind(new Date(Date.now() - 60_000).toISOString(), contentId)
      .run();
    await sweepExpiredPrivacyRows(testDb);
    expect(await storedRow(contentId)).toBeNull();
  });

  it("tombstones an expired row instead of serving it", async () => {
    const { contentId } = await seedSingleUseKey();
    await testDb
      .prepare("UPDATE wrapped_keys SET expires_at = ? WHERE content_id = ?")
      .bind(new Date(Date.now() - 60_000).toISOString(), contentId)
      .run();
    const gone = await SELF.fetch(await signedGetUrl(contentId));
    expect(gone.status).toBe(410);
    expect(await storedRow(contentId)).toBeNull();
  });

  it("a sender burn still destroys a key that was already fetched", async () => {
    // Retention must not cost the sender the ability to destroy the copy.
    // Burn — not transmission — is what removes it before TTL, and it must
    // still work on a row that has already been read.
    const { contentId } = await seedSingleUseKey();
    expect((await SELF.fetch(await signedGetUrl(contentId))).status).toBe(200);
    expect(await storedRow(contentId)).not.toBeNull();

    const meta = {
      timestamp_ms: Date.now(),
      request_id: base64Encode(crypto.getRandomValues(new Uint8Array(32)))
        .replaceAll("+", "-")
        .replaceAll("/", "_")
        .replaceAll("=", ""),
    };
    const burnMessage = canonicalBurnBytes({
      user_id: senderId,
      ...meta,
      scope: "single",
      target: { content_id: contentId },
    });
    const burn = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "DELETE",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        scope: "single",
        user_id: senderId,
        ...meta,
        target_content_id: contentId,
        burn_signature_b64: await signEd25519(senderSigningKey, burnMessage),
      }),
    });
    expect(burn.status).toBe(200);
    expect((await burn.json() as { deleted_count: number }).deleted_count).toBe(1);
    expect(await storedRow(contentId)).toBeNull();
    expect((await SELF.fetch(await signedGetUrl(contentId))).status).toBe(404);
  });
});
