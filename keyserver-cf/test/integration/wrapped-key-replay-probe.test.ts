// PROBE (row-7 verification lane) — what a *naive* retry gets.
//
// `wrapped-key-reservation.test.ts` retries with a FRESH signature, on the
// stated assumption that this is "what a real retrying client does". This file
// probes the other half: a transport-level retry that replays the IDENTICAL
// signed URL, which is what any generic HTTP retry (reqwest middleware, a
// proxy, a user hitting refresh) produces. If the server refuses that, the
// non-destructive read only saves the message for clients that re-sign.
import { SELF, env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import {
  canonicalWrappedKeyGetBytes,
  canonicalWrappedKeyPostBytes,
} from "../../src/lib/canonical.js";
import { base64Encode, registerTestUser, signEd25519 } from "./helpers.js";

const testDb = (env as unknown as { DB: D1Database }).DB;

describe("row-7 probe: identical-signature retry after a lost response", () => {
  let senderId: string;
  let senderSigningKey: CryptoKey;
  let recipientId: string;
  let recipientSigningKey: CryptoKey;

  beforeEach(async () => {
    senderId = `probe-sender-${Math.random().toString(36).slice(2, 8)}`;
    senderSigningKey = (await registerTestUser(SELF, senderId)).signingKey;
    recipientId = `probe-recipient-${Math.random().toString(36).slice(2, 8)}`;
    recipientSigningKey = (await registerTestUser(SELF, recipientId)).signingKey;
  });

  async function seed(): Promise<{ contentId: string; blob: string }> {
    const body = {
      content_id: `probe-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
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
    };
    const message = canonicalWrappedKeyPostBytes(body as never);
    const response = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        ...body,
        sender_signature_b64: await signEd25519(senderSigningKey, message),
      }),
    });
    expect(response.status).toBe(201);
    return { contentId: body.content_id, blob: body.wrapped_share_blob };
  }

  async function signedGetUrl(contentId: string, timestampMs: number): Promise<string> {
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

  it("replaying the SAME signed URL after a lost response", async () => {
    const { contentId, blob } = await seed();
    const url = await signedGetUrl(contentId, Date.now());

    const first = await SELF.fetch(url);
    expect(first.status).toBe(200);
    await first.body?.cancel(); // response lost — recipient holds nothing

    // The row survives (T6-K5). The question is whether the recipient can
    // reach it with the request bytes it already has.
    const row = await testDb
      .prepare("SELECT wrapped_share_blob FROM wrapped_keys WHERE content_id = ?")
      .bind(contentId)
      .first<{ wrapped_share_blob: string }>();
    expect(row?.wrapped_share_blob).toBe(blob);

    // MEASURED: 409. The receipt row is keyed on
    // (requester_id, request_digest) where request_digest is SHA-256 of the
    // canonical message — whose only varying field is the millisecond
    // timestamp. Replaying the same bytes therefore collides with the receipt
    // the FIRST (lost) attempt already wrote, and is refused.
    //
    // This is pinned deliberately. T6-K5 keeps the row alive, but the row is
    // only reachable by a client that RE-SIGNS. If this ever changes to 200,
    // the recovery contract has widened and the change must be conscious.
    const retry = await SELF.fetch(url);
    expect(retry.status).toBe(409);
    expect(await retry.text()).toContain("already consumed");
  });

  it("re-signing with a fresh timestamp always recovers the message", async () => {
    const { contentId, blob } = await seed();
    const lost = await SELF.fetch(await signedGetUrl(contentId, Date.now()));
    expect(lost.status).toBe(200);
    await lost.body?.cancel();

    const fresh = await SELF.fetch(await signedGetUrl(contentId, Date.now() + 1));
    expect(fresh.status).toBe(200);
    expect(((await fresh.json()) as { wrapped_share_blob: string }).wrapped_share_blob).toBe(blob);
  });
});
