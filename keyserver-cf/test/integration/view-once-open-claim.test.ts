import { SELF, env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import {
  canonicalWrappedKeyOpenClaimBytes,
  canonicalWrappedKeyPostBytes,
} from "../../src/lib/canonical.js";
import { base64Encode, registerTestUser, signEd25519 } from "./helpers.js";

const testDb = (env as unknown as { DB: D1Database }).DB;

describe("TASK 0557 view-once opened server claim", () => {
  let senderId: string;
  let senderSigningKey: CryptoKey;
  let recipientId: string;
  let recipientSigningKey: CryptoKey;
  let sequence = 0;

  beforeEach(async () => {
    senderId = `t0557-sender-${Math.random().toString(36).slice(2, 8)}`;
    senderSigningKey = (await registerTestUser(SELF, senderId)).signingKey;
    recipientId = `t0557-recipient-${Math.random().toString(36).slice(2, 8)}`;
    recipientSigningKey = (await registerTestUser(SELF, recipientId)).signingKey;
  });

  async function seedSingleUseRecord(): Promise<string> {
    const body = {
      content_id: `t0557-record-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
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
    const message = canonicalWrappedKeyPostBytes(body);
    const response = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        ...body,
        sender_signature_b64: await signEd25519(senderSigningKey, message),
      }),
    });
    expect(response.status).toBe(201);
    return body.content_id;
  }

  async function openedClaim(contentId: string): Promise<Response> {
    const timestampMs = Date.now() + sequence++;
    const requestId = `${String(sequence).padStart(2, "0")}AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA`;
    expect(requestId).toHaveLength(43);
    const message = canonicalWrappedKeyOpenClaimBytes({
      recipient_id: recipientId,
      content_id: contentId,
      timestamp_ms: timestampMs,
      request_id: requestId,
    });
    return SELF.fetch(`http://test/v1/wrapped-keys/${encodeURIComponent(contentId)}/opened`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        recipient_id: recipientId,
        timestamp_ms: timestampMs,
        request_id: requestId,
        open_signature_b64: await signEd25519(recipientSigningKey, message),
      }),
    });
  }

  it("first claim succeeds and a fresh second claim for the same record fails", async () => {
    const contentId = await seedSingleUseRecord();
    expect(
      await testDb
        .prepare("SELECT COUNT(*) AS count FROM wrapped_keys WHERE content_id = ?")
        .bind(contentId)
        .first<{ count: number }>(),
    ).toEqual({ count: 1 });

    const first = await openedClaim(contentId);
    expect(first.status).toBe(200);
    expect(await first.json()).toEqual({ content_id: contentId, opened: true });
    expect(
      await testDb
        .prepare("SELECT COUNT(*) AS count FROM wrapped_keys WHERE content_id = ?")
        .bind(contentId)
        .first<{ count: number }>(),
    ).toEqual({ count: 0 });
    expect(
      await testDb
        .prepare("SELECT COUNT(*) AS count FROM wrapped_key_open_receipts WHERE content_id = ?")
        .bind(contentId)
        .first<{ count: number }>(),
    ).toEqual({ count: 1 });

    const second = await openedClaim(contentId);
    expect(second.status).toBe(409);
    expect(await second.text()).toContain("already opened");

    console.log(
      "TASK0557 server_first_claim_status=200 server_second_claim_status=409 opened_receipts=1 wrapped_keys_remaining=0",
    );
  });
});
