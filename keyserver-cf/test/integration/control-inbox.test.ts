import { SELF, env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import type { Env } from "../../src/env.js";
import {
  handleControlInboxGet,
  handleControlInboxPost,
} from "../../src/endpoints/control-inbox.js";
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
  scopeId = `scope-${seq++}`,
): Promise<Record<string, unknown>> {
  const bundle = new TextEncoder().encode(`bundle-${seq++}`);
  const bundleHash = new Uint8Array(
    await crypto.subtle.digest("SHA-256", bundle),
  );
  const fields = {
    sender_id: senderId,
    recipient_id: recipientId,
    scope_id: scopeId,
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
    const firstJson = (await first.json()) as {
      id: string;
      inbox_eviction_count: number;
    };
    expect(firstJson.inbox_eviction_count).toBe(0);

    const retry = await post(body);
    expect(retry.status).toBe(200);
    const retryJson = (await retry.json()) as {
      id: string;
      replayed: boolean;
      inbox_eviction_count: number;
    };
    expect(retryJson).toMatchObject({
      id: firstJson.id,
      replayed: true,
      inbox_eviction_count: 0,
    });

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
    expect(first.status).toBe(201);
    const firstJson = (await first.json()) as { id: string };
    expect(firstJson.id).toEqual(expect.any(String));
    expect(firstJson.id.length).toBeGreaterThan(0);
    const enqueued = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM control_inbox WHERE recipient_id = ?",
    )
      .bind(recipientId)
      .first<{ count: number }>();
    expect(enqueued?.count).toBe(1);
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
    // Contract change (2026-07-26 audit). This previously asserted 201: a full
    // recipient inbox was made to fit by evicting its oldest undelivered row,
    // whoever had sent it. Because registration is open, that turned the cap
    // into a cross-account deletion primitive -- an attacker could destroy an
    // offline victim's pending SKDM/control state, and the dependent protected
    // messages with it, silently.
    //
    // The storage bound is unchanged. It is now enforced by refusing the newest
    // row instead of destroying somebody else's oldest, and the refusal is an
    // explicit, documented code the client already handles
    // (`recipient_inbox_full` in crates/keystore/src/client.rs) rather than
    // silence. A sender is still never permanently blocked: it keeps its own
    // per-pair allowance, and reserved headroom keeps a first contact
    // deliverable -- see control-inbox-cross-sender.test.ts.
    expect(res.status).toBe(429);
    expect(await res.json()).toMatchObject({
      error: "recipient_inbox_full",
      scope: "recipient",
    });
    const after = await env.DB.prepare(
      `SELECT COUNT(*) AS count FROM control_inbox WHERE recipient_id = ?`,
    )
      .bind(recipientId)
      .first<{ count: number }>();
    // Nothing was deleted to make room, and nothing was added.
    expect(after?.count).toBe(512);
  });

  it("inbox eviction observability under D1 512-row backstop", async () => {
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
       SELECT randomblob(16), ?, 'b87-filler-' || x, 'b87-backstop', x'01', ?, ? FROM cnt`,
    )
      .bind(recipientId, now + 3600, now)
      .run();

    await expect(
      env.DB.prepare(
        `INSERT INTO control_inbox
           (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at)
         VALUES (randomblob(16), ?, ?, 'b87-direct-trigger', x'01', ?, ?)`,
      )
        .bind(recipientId, senderId, now + 3600, now + 1)
        .run(),
    ).rejects.toThrow(/control inbox recipient quota exceeded/u);

    const res = await post(
      await signedPostBody(
        senderId,
        recipientId,
        sender.signingKey,
        Date.now(),
        "b87-public-post",
      ),
    );
    expect(res.status).toBe(429);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body).toMatchObject({
      error: "recipient_inbox_full",
      scope: "recipient",
    });
    expect(body).not.toHaveProperty("inbox_eviction_count");

    const receipt = await env.DB.prepare(
      `SELECT COUNT(*) AS count FROM control_inbox_requests WHERE sender_id = ?`,
    )
      .bind(senderId)
      .first<{ count: number }>();
    expect(receipt?.count).toBe(0);

    const after = await env.DB.prepare(
      `SELECT COUNT(*) AS count FROM control_inbox WHERE recipient_id = ?`,
    )
      .bind(recipientId)
      .first<{ count: number }>();
    expect(after?.count).toBe(512);
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
    // Same contract change on the per-pair cap, which is the one a normal
    // conversation actually reaches (32 undelivered messages to one person).
    // The sender is never blocked; the stalest row makes way.
    expect(blocked.status).toBe(201);
    const after = await env.DB.prepare(
      `SELECT COUNT(*) AS count FROM control_inbox
        WHERE recipient_id = ? AND sender_id = ?`,
    )
      .bind(recipientId, senderId)
      .first<{ count: number }>();
    expect(after?.count).toBe(32);

    const legitimateId = userId("legitimate");
    const legitimate = await registerTestUser(SELF, legitimateId);
    const admitted = await post(
      await signedPostBody(legitimateId, recipientId, legitimate.signingKey),
    );
    expect(admitted.status).toBe(201);
  });

  it("reports same-sender inbox eviction count on success and replay", async () => {
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
       SELECT randomblob(16), ?, ?, 'pair-recycle-signal', x'01', ?, ? + x FROM cnt`,
    )
      .bind(recipientId, senderId, now + 3600, now)
      .run();

    const body = await signedPostBody(
      senderId,
      recipientId,
      sender.signingKey,
    );
    const first = await post(body);
    expect(first.status).toBe(201);
    const firstJson = (await first.json()) as {
      id: string;
      inbox_eviction_count: number;
    };
    expect(firstJson.inbox_eviction_count).toBe(1);

    const retry = await post(body);
    expect(retry.status).toBe(200);
    expect(await retry.json()).toMatchObject({
      id: firstJson.id,
      replayed: true,
      inbox_eviction_count: 1,
    });

    const receipt = await env.DB.prepare(
      `SELECT inbox_eviction_count FROM control_inbox_requests
        WHERE sender_id = ?`,
    )
      .bind(senderId)
      .first<{ inbox_eviction_count: number }>();
    expect(receipt?.inbox_eviction_count).toBe(1);

    const held = await env.DB.prepare(
      `SELECT COUNT(*) AS count FROM control_inbox
        WHERE recipient_id = ? AND sender_id = ?`,
    )
      .bind(recipientId, senderId)
      .first<{ count: number }>();
    expect(held?.count).toBe(32);
  });
});

describe("POST /v1/control-inbox 429 disambiguation", () => {
  // Integration traffic runs with effectively unlimited native limiters
  // (see vitest.config.ts), so drive the throttled branch with a fake
  // binding — the same deterministic-fake convention the rate-limit unit
  // tests use.
  function deniedRateLimitEnv(): Env {
    return {
      RATE_LIMIT_1200: {
        limit: async () => ({ success: false }),
      },
      DB: {
        prepare() {
          throw new Error("must not reach the database when throttled");
        },
      },
    } as unknown as Env;
  }

  it("still reports a genuine per-IP throttle as rate_limited", async () => {
    const res = await handleControlInboxPost(
      new Request("http://test/v1/control-inbox", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "cf-connecting-ip": "203.0.113.7",
        },
        body: JSON.stringify({ sender_id: "sender" }),
      }),
      deniedRateLimitEnv(),
    );
    expect(res.status).toBe(429);
    expect(res.headers.get("retry-after")).toBe("60");
    expect(await res.json()).toEqual({ error: "rate_limited" });
  });

  it("never labels a full recipient inbox as rate_limited", async () => {
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
       SELECT randomblob(16), ?, ?, 'label-test', x'01', ?, ? FROM cnt`,
    )
      .bind(recipientId, senderId, now + 3600, now)
      .run();

    const res = await post(
      await signedPostBody(senderId, recipientId, sender.signingKey),
    );
    // A full inbox is now absorbed by eviction rather than refused, so the
    // mislabel this test guarded against can no longer be produced at all.
    // The guarantee it encodes is unchanged and stronger: reaching the cap
    // never tells the sender to slow down, because it never refuses them.
    expect(res.status).toBe(201);
    expect(await res.text()).not.toContain("rate_limited");
  });
});

describe("control-inbox error responses", () => {
  it("logs internal GET failures without leaking exception text", async () => {
    const marker = "secret database diagnostic";
    const fakeEnv = {
      DB: {
        prepare(sql: string) {
          if (sql.includes("worker_schema_capabilities")) {
            return {
              bind() {
                return {
                  async first() {
                    return { version: 1 };
                  },
                };
              },
            };
          }
          if (sql.includes("LIMIT 0")) {
            return {
              async all() {
                return { results: [] };
              },
            };
          }
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
