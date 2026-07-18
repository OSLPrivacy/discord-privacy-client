import { SELF, env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import type { Env } from "../../src/env.js";
import { handleControlInboxGet } from "../../src/endpoints/control-inbox.js";
import { canonicalControlInboxPostBytes } from "../../src/lib/canonical.js";
import {
  base64Encode,
  registerTestUser,
  signEd25519,
} from "./helpers.js";

let seq = 0;
const userId = (prefix: string) =>
  `${prefix}-${Date.now().toString(36)}-${seq++}`;

async function signedPostBody(
  senderId: string,
  recipientId: string,
  signingKey: CryptoKey,
  timestampMs = Date.now(),
): Promise<Record<string, unknown>> {
  const bundle = new TextEncoder().encode(`bundle-${seq++}`);
  const bundleHash = new Uint8Array(
    await crypto.subtle.digest("SHA-256", bundle),
  );
  const fields = {
    sender_id: senderId,
    recipient_id: recipientId,
    scope_id: `scope-${seq++}`,
    timestamp_ms: timestampMs,
    bundle_sha256: bundleHash,
  };
  return {
    sender_id: senderId,
    recipient_id: recipientId,
    scope_id: fields.scope_id,
    timestamp_ms: timestampMs,
    bundle_b64: base64Encode(bundle),
    signature_b64: await signEd25519(
      signingKey,
      canonicalControlInboxPostBytes(fields),
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

describe("POST /v1/control-inbox hardening", () => {
  it("rejects malformed base64 without an internal error", async () => {
    const res = await post({
      sender_id: "sender",
      recipient_id: "recipient",
      scope_id: "scope",
      timestamp_ms: Date.now(),
      bundle_b64: "A",
      signature_b64: "AAAA",
    });
    expect(res.status).toBe(400);
  });
  it("requires a registered recipient after authenticating the sender", async () => {
    const senderId = userId("sender");
    const sender = await registerTestUser(SELF, senderId);
    const res = await post(
      await signedPostBody(
        senderId,
        userId("missing-recipient"),
        sender.signingKey,
      ),
    );
    expect(res.status).toBe(404);
  });

  it("deduplicates an exact signed retry and returns the original id", async () => {
    const senderId = userId("sender");
    const recipientId = userId("recipient");
    const sender = await registerTestUser(SELF, senderId);
    await registerTestUser(SELF, recipientId);
    const body = await signedPostBody(
      senderId,
      recipientId,
      sender.signingKey,
    );

    const first = await post(body);
    expect(first.status).toBe(201);
    const firstJson = (await first.json()) as { id: string };

    const retry = await post(body);
    expect(retry.status).toBe(200);
    const retryJson = (await retry.json()) as {
      id: string;
      replayed: boolean;
    };
    expect(retryJson).toMatchObject({ id: firstJson.id, replayed: true });

    const count = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM control_inbox WHERE recipient_id = ?",
    )
      .bind(recipientId)
      .first<{ count: number }>();
    expect(count?.count).toBe(1);
  });

  it("does not re-enqueue a captured request after its item was applied", async () => {
    const senderId = userId("sender");
    const recipientId = userId("recipient");
    const sender = await registerTestUser(SELF, senderId);
    await registerTestUser(SELF, recipientId);
    const body = await signedPostBody(
      senderId,
      recipientId,
      sender.signingKey,
    );
    const first = await post(body);
    const firstJson = (await first.json()) as { id: string };
    await env.DB.prepare("DELETE FROM control_inbox WHERE recipient_id = ?")
      .bind(recipientId)
      .run();

    const replay = await post(body);
    expect(replay.status).toBe(200);
    expect(await replay.json()).toMatchObject({
      id: firstJson.id,
      replayed: true,
    });
    const count = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM control_inbox WHERE recipient_id = ?",
    )
      .bind(recipientId)
      .first<{ count: number }>();
    expect(count?.count).toBe(0);
  });

  it("caps pending opaque data per recipient", async () => {
    const senderId = userId("sender");
    const recipientId = userId("recipient");
    const sender = await registerTestUser(SELF, senderId);
    await registerTestUser(SELF, recipientId);
    const now = Math.floor(Date.now() / 1000);
    await env.DB.prepare(
      `WITH RECURSIVE cnt(x) AS (
         VALUES(1) UNION ALL SELECT x + 1 FROM cnt WHERE x < 512
       )
       INSERT INTO control_inbox
         (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at)
       SELECT randomblob(16), ?, ? || x, 'quota-test', x'01', ?, ? FROM cnt`,
    )
      .bind(recipientId, senderId, now + 3600, now)
      .run();

    const res = await post(
      await signedPostBody(senderId, recipientId, sender.signingKey),
    );
    expect(res.status).toBe(429);
  });

  it("prevents one sender from consuming a recipient's full inbox", async () => {
    const senderId = userId("sender");
    const recipientId = userId("recipient");
    const sender = await registerTestUser(SELF, senderId);
    await registerTestUser(SELF, recipientId);
    const now = Math.floor(Date.now() / 1000);
    await env.DB.prepare(
      `WITH RECURSIVE cnt(x) AS (
         VALUES(1) UNION ALL SELECT x + 1 FROM cnt WHERE x < 32
       )
       INSERT INTO control_inbox
         (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at)
       SELECT randomblob(16), ?, ?, 'pair-quota', x'01', ?, ? FROM cnt`,
    )
      .bind(recipientId, senderId, now + 3600, now)
      .run();

    const blocked = await post(
      await signedPostBody(senderId, recipientId, sender.signingKey),
    );
    expect(blocked.status).toBe(429);

    const legitimateId = userId("legitimate");
    const legitimate = await registerTestUser(SELF, legitimateId);
    const admitted = await post(
      await signedPostBody(legitimateId, recipientId, legitimate.signingKey),
    );
    expect(admitted.status).toBe(201);
  });
});

describe("control-inbox error responses", () => {
  it("logs internal GET failures without leaking exception text", async () => {
    const marker = "secret database diagnostic";
    const fakeEnv = {
      DB: {
        prepare() {
          throw new Error(marker);
        },
      },
    } as unknown as Env;
    const log = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      const res = await handleControlInboxGet(
        new Request(
          `http://test/v1/control-inbox/alice?ts=${Date.now()}&sig=AAAA`,
        ),
        fakeEnv,
        "alice",
      );
      expect(res.status).toBe(500);
      const text = await res.text();
      expect(text).toContain("internal error");
      expect(text).not.toContain(marker);
      expect(log).toHaveBeenCalled();
    } finally {
      log.mockRestore();
    }
  });
});
