import { SELF, env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import {
  canonicalBurnBytes,
  canonicalWrappedKeyPostBytes,
  canonicalWrappedKeyGetBytes,
} from "../../src/lib/canonical.js";
import { fetchWrappedKeyAuthenticated } from "../../src/lib/db.js";
import {
  base64Encode,
  registerTestUser,
  signEd25519,
  TEST_ADMIN_TOKEN,
} from "./helpers.js";

const STUB_SHARE_BLOB = base64Encode(new Uint8Array([1, 2, 3, 4]));
const FUTURE_ISO = () => new Date(Date.now() + 5 * 60_000).toISOString();
const PAST_ISO = () => new Date(Date.now() - 1000).toISOString();
const testDb = (env as unknown as { DB: D1Database }).DB;

// Default fixture recipient used by POST/burn tests. Recipient existence is a
// storage boundary now, so seed this identity once for the file.
await registerTestUser(SELF, "bob");

function uniqueContentId(prefix = "msg"): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function burnMeta(timestamp_ms = Date.now()): { timestamp_ms: number; request_id: string } {
  const request_id = base64Encode(crypto.getRandomValues(new Uint8Array(32)))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
  return { timestamp_ms, request_id };
}

function validBody(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    content_id: uniqueContentId(),
    content_type: "text",
    sender_id: "alice",
    recipient_id: "bob",
    session_version: 1,
    share_index: 0,
    wrapped_share_blob: STUB_SHARE_BLOB,
    blob_version: 1,
    single_use: false,
    expires_at: FUTURE_ISO(),
    timestamp_ms: Date.now(),
    ...overrides,
  };
}

async function postSignedWrappedKey(
  body: Record<string, unknown>,
  signingKey: CryptoKey,
): Promise<Response> {
  const message = canonicalWrappedKeyPostBytes({
    content_id: body.content_id as string,
    content_type: body.content_type as string,
    system_message_kind: (body.system_message_kind as string | undefined) ?? null,
    sender_id: body.sender_id as string,
    recipient_id: body.recipient_id as string,
    session_version: body.session_version as number,
    share_index: body.share_index as number,
    wrapped_share_blob: body.wrapped_share_blob as string,
    blob_version: body.blob_version as number,
    single_use: body.single_use as boolean,
    display_duration_seconds:
      (body.display_duration_seconds as number | undefined) ?? null,
    expires_at: body.expires_at as string,
    timestamp_ms: body.timestamp_ms as number,
  });
  return SELF.fetch("http://test/v1/wrapped-keys", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      ...body,
      sender_signature_b64: await signEd25519(signingKey, message),
    }),
  });
}

describe("POST /v1/wrapped-keys", () => {
  let senderId: string;
  let senderSigningKey: CryptoKey;

  beforeEach(async () => {
    senderId = `sender-${Math.random().toString(36).slice(2, 8)}`;
    senderSigningKey = (await registerTestUser(SELF, senderId)).signingKey;
  });

  it("rejects an unsigned upload", async () => {
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(validBody({ sender_id: senderId })),
    });
    expect(res.status).toBe(400);
  });

  it("rejects an invalid identity signature even with an operator bearer", async () => {
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${TEST_ADMIN_TOKEN}`,
      },
      body: JSON.stringify({
        ...validBody({ sender_id: senderId }),
        sender_signature_b64: base64Encode(new Uint8Array(64).fill(0xaa)),
      }),
    });
    expect(res.status).toBe(401);
  });

  it("rejects a signature whose decoded length is not Ed25519-sized", async () => {
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        ...validBody({ sender_id: senderId }),
        sender_signature_b64: base64Encode(new Uint8Array(63)),
      }),
    });
    expect(res.status).toBe(400);
    expect(await res.json()).toEqual({
      error: "sender_signature_b64 must decode to 64 bytes",
    });
  });

  it("201s on a valid identity-signed insert without a shared bearer", async () => {
    const body = validBody({ sender_id: senderId });
    const res = await postSignedWrappedKey(body, senderSigningKey);
    expect(res.status).toBe(201);
    expect(await res.json()).toEqual({ content_id: body.content_id });
  });

  it("409s on duplicate content_id", async () => {
    const body = validBody({ sender_id: senderId });
    await postSignedWrappedKey(body, senderSigningKey);
    const res = await postSignedWrappedKey(body, senderSigningKey);
    expect(res.status).toBe(409);
  });

  it("400s missing display_duration_seconds when single_use=true", async () => {
    const res = await postSignedWrappedKey(
      validBody({ sender_id: senderId, single_use: true }),
      senderSigningKey,
    );
    expect(res.status).toBe(400);
  });

  it("400s display_duration_seconds present when single_use=false", async () => {
    const res = await postSignedWrappedKey(
      validBody({ sender_id: senderId, display_duration_seconds: 5 }),
      senderSigningKey,
    );
    expect(res.status).toBe(400);
  });

  it("400s system_message_kind on non-system content_type", async () => {
    const res = await postSignedWrappedKey(
      validBody({ sender_id: senderId, system_message_kind: "burn-alert" }),
      senderSigningKey,
    );
    expect(res.status).toBe(400);
  });

  it("201s system content_type with allowed kind", async () => {
    const res = await postSignedWrappedKey(
      validBody({
        sender_id: senderId,
        content_type: "system",
        system_message_kind: "burn-alert",
      }),
      senderSigningKey,
    );
    expect(res.status).toBe(201);
  });

  it("rejects stale signatures", async () => {
    const res = await postSignedWrappedKey(
      validBody({ sender_id: senderId, timestamp_ms: Date.now() - 6 * 60_000 }),
      senderSigningKey,
    );
    expect(res.status).toBe(401);
  });

  it("rejects wrapped-key retention beyond seven days", async () => {
    const res = await postSignedWrappedKey(
      validBody({
        sender_id: senderId,
        expires_at: new Date(Date.now() + 8 * 24 * 60 * 60_000).toISOString(),
      }),
      senderSigningKey,
    );
    expect(res.status).toBe(400);
    expect(await res.json()).toEqual({ error: "expires_at cannot exceed 7 days" });
  });

  it("rejects past and noncanonical expiry strings", async () => {
    for (const expires_at of [PAST_ISO(), "July 18, 2026", "2026-07-18T00:00:00Z"]) {
      const res = await postSignedWrappedKey(
        validBody({ sender_id: senderId, expires_at }),
        senderSigningKey,
      );
      expect(res.status, expires_at).toBe(400);
    }
  });

  it("rejects a non-SQLite expiry at the database boundary", async () => {
    await expect(
      testDb
        .prepare(
          `INSERT INTO wrapped_keys
             (content_id, content_type, sender_id, recipient_id, session_version,
              share_index, wrapped_share_blob, blob_version, single_use,
              expires_at, created_at)
           VALUES (?, 'text', ?, 'bob', 1, 0, 'AQ==', 1, 0, ?, ?)`,
        )
        .bind(
          uniqueContentId("invalid-expiry"),
          senderId,
          "July 18, 2026",
          new Date().toISOString(),
        )
        .run(),
    ).rejects.toThrow(/expiry is not SQLite-compatible/);
  });

  it("rejects an oversized decoded wrapped share", async () => {
    const res = await postSignedWrappedKey(
      validBody({
        sender_id: senderId,
        wrapped_share_blob: base64Encode(new Uint8Array(64 * 1024 + 1)),
      }),
      senderSigningKey,
    );
    expect(res.status).toBe(400);
  });

  it("rejects a wrapped key for an unregistered recipient", async () => {
    const res = await postSignedWrappedKey(
      validBody({
        sender_id: senderId,
        recipient_id: `missing-${Math.random().toString(36).slice(2)}`,
      }),
      senderSigningKey,
    );
    expect(res.status).toBe(404);
  });

  for (const [field, overrides] of [
    ["session_version", { session_version: 0x1_0000_0000 }],
    ["share_index", { share_index: 0x1_0000_0000 }],
    ["blob_version", { blob_version: 0x1_0000_0000 }],
    [
      "display_duration_seconds",
      { single_use: true, display_duration_seconds: 0x1_0000_0000 },
    ],
  ] as const) {
    it(`rejects out-of-range canonical u32 field ${field}`, async () => {
      const res = await SELF.fetch("http://test/v1/wrapped-keys", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          ...validBody({ sender_id: senderId, ...overrides }),
          sender_signature_b64: base64Encode(new Uint8Array(64)),
        }),
      });
      expect(res.status).toBe(400);
    });
  }

  it("enforces the live-row sender quota at the D1 boundary", async () => {
    const prefix = uniqueContentId("quota");
    await testDb
      .prepare(
        `WITH RECURSIVE cnt(x) AS (
           VALUES(1) UNION ALL SELECT x + 1 FROM cnt WHERE x < 2048
         )
         INSERT INTO wrapped_keys
           (content_id, content_type, sender_id, recipient_id, session_version,
            share_index, wrapped_share_blob, blob_version, single_use,
            expires_at, created_at)
         SELECT ? || '-' || x, 'text', ?, 'quota-recipient-' || x,
                1, x, 'AQ==', 1, 0, ?, ?
           FROM cnt`,
      )
      .bind(prefix, senderId, FUTURE_ISO(), new Date().toISOString())
      .run();
    const res = await postSignedWrappedKey(
      validBody({ sender_id: senderId }),
      senderSigningKey,
    );
    expect(res.status).toBe(429);
  });

  it("enforces the aggregate physical-storage ceiling", async () => {
    await testDb
      .prepare(
        `UPDATE wrapped_key_storage_usage
            SET decoded_bytes = 1073741824
          WHERE singleton = 1`,
      )
      .run();
    try {
      const res = await postSignedWrappedKey(
        validBody({ sender_id: senderId }),
        senderSigningKey,
      );
      expect(res.status).toBe(429);
    } finally {
      // Restore the exact counter so this deliberate boundary probe cannot
      // influence later cases in this shared test database.
      await testDb
        .prepare(
          `UPDATE wrapped_key_storage_usage
              SET row_count = (SELECT COUNT(*) FROM wrapped_keys),
                  decoded_bytes = (
                    SELECT COALESCE(
                      SUM(
                        (length(wrapped_share_blob) * 3 / 4) -
                        CASE
                          WHEN substr(wrapped_share_blob, -2) = '==' THEN 2
                          WHEN substr(wrapped_share_blob, -1) = '=' THEN 1
                          ELSE 0
                        END
                      ),
                      0
                    )
                    FROM wrapped_keys
                  )
            WHERE singleton = 1`,
        )
        .run();
    }
  });

  it("rejects a body changed after signing", async () => {
    const body = validBody({ sender_id: senderId });
    const message = canonicalWrappedKeyPostBytes({
      content_id: body.content_id as string,
      content_type: body.content_type as string,
      system_message_kind: null,
      sender_id: senderId,
      recipient_id: body.recipient_id as string,
      session_version: body.session_version as number,
      share_index: body.share_index as number,
      wrapped_share_blob: body.wrapped_share_blob as string,
      blob_version: body.blob_version as number,
      single_use: body.single_use as boolean,
      display_duration_seconds: null,
      expires_at: body.expires_at as string,
      timestamp_ms: body.timestamp_ms as number,
    });
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        ...body,
        recipient_id: "mallory",
        sender_signature_b64: await signEd25519(senderSigningKey, message),
      }),
    });
    expect(res.status).toBe(401);
  });

  it("enforces the per-sender live-row quota at the D1 write boundary", async () => {
    await testDb
      .prepare(
        `WITH RECURSIVE seq(n) AS (
           SELECT 1
           UNION ALL
           SELECT n + 1 FROM seq WHERE n < 2048
         )
         INSERT INTO wrapped_keys (
           content_id, content_type, system_message_kind,
           sender_id, recipient_id, session_version, share_index,
           wrapped_share_blob, blob_version, single_use,
           display_duration_seconds, expires_at, created_at
         )
         SELECT ?1 || '-quota-' || n, 'text', NULL,
                ?1, 'quota-recipient-' || n,
                1, n, ?2, 1, 0, NULL, ?3, ?4
           FROM seq`,
      )
      .bind(senderId, STUB_SHARE_BLOB, FUTURE_ISO(), new Date().toISOString())
      .run();

    const denied = await postSignedWrappedKey(
      validBody({ sender_id: senderId }),
      senderSigningKey,
    );
    expect(denied.status).toBe(429);

    // Quota is intentionally live-only: an expired row waiting for the hourly
    // physical sweep cannot keep a legitimate sender locked out.
    await testDb
      .prepare("UPDATE wrapped_keys SET expires_at = ? WHERE sender_id = ?")
      .bind(PAST_ISO(), senderId)
      .run();
    const admitted = await postSignedWrappedKey(
      validBody({ sender_id: senderId }),
      senderSigningKey,
    );
    expect(admitted.status).toBe(201);
  });
});

describe("GET /v1/wrapped-keys/:content_id", () => {
  let senderId: string;
  let senderSigningKey: CryptoKey;
  let recipientId: string;
  let recipientSigningKey: CryptoKey;
  let recipientPublicKeyB64: string;
  let signedGetSequence = 0;

  beforeEach(async () => {
    senderId = `sender-${Math.random().toString(36).slice(2, 8)}`;
    senderSigningKey = (await registerTestUser(SELF, senderId)).signingKey;
    recipientId = `recipient-${Math.random().toString(36).slice(2, 8)}`;
    const recipient = await registerTestUser(SELF, recipientId);
    recipientSigningKey = recipient.signingKey;
    recipientPublicKeyB64 = recipient.publicKeyB64;
  });

  async function seedWrappedKey(
    overrides: Partial<Record<string, unknown>> = {},
  ): Promise<Record<string, unknown>> {
    const body = validBody({
      sender_id: senderId,
      recipient_id: recipientId,
      ...overrides,
    });
    const response = await postSignedWrappedKey(body, senderSigningKey);
    expect(response.status).toBe(201);
    return body;
  }

  async function signedGetUrl(
    contentId: string,
    signedRecipientId = recipientId,
    requesterId = signedRecipientId,
    signingKey = recipientSigningKey,
    timestampMs = Date.now() + signedGetSequence++,
  ): Promise<string> {
    const message = canonicalWrappedKeyGetBytes({
      requester_id: requesterId,
      recipient_id: signedRecipientId,
      content_id: contentId,
      timestamp_ms: timestampMs,
    });
    const sig = await signEd25519(signingKey, message);
    const q = new URLSearchParams({
      requester_id: requesterId,
      recipient_id: signedRecipientId,
      ts: String(timestampMs),
      sig,
    });
    return `http://test/v1/wrapped-keys/${encodeURIComponent(contentId)}?${q}`;
  }

  it("404s for unknown content_id", async () => {
    const res = await SELF.fetch(await signedGetUrl("unknown-id"));
    expect(res.status).toBe(404);
  });

  it("requires fresh recipient authorization for reusable rows", async () => {
    const body = await seedWrappedKey();
    const unauthenticated = await SELF.fetch(
      `http://test/v1/wrapped-keys/${body.content_id}`,
    );
    expect(unauthenticated.status).toBe(401);
    const res = await SELF.fetch(await signedGetUrl(body.content_id as string));
    expect(res.status).toBe(200);
    const j = (await res.json()) as Record<string, unknown>;
    expect(j.content_id).toBe(body.content_id);
    expect(j.sender_id).toBe(body.sender_id);
    expect(j.wrapped_share_blob).toBe(body.wrapped_share_blob);
    expect(j.single_use).toBe(false);
  });

  it("410s a past-expiry row and tombstones it", async () => {
    const body = await seedWrappedKey();
    await env.DB.prepare("UPDATE wrapped_keys SET expires_at = ? WHERE content_id = ?")
      .bind(PAST_ISO(), body.content_id)
      .run();
    const r1 = await SELF.fetch(await signedGetUrl(body.content_id as string));
    expect(r1.status).toBe(410);
    // Subsequent fetch: row tombstoned → 404
    const r2 = await SELF.fetch(await signedGetUrl(body.content_id as string));
    expect(r2.status).toBe(404);
  });

  it("single_use row is consumed on first read", async () => {
    const body = await seedWrappedKey({
      single_use: true,
      display_duration_seconds: 10,
    });
    const r1 = await SELF.fetch(await signedGetUrl(body.content_id as string));
    expect(r1.status).toBe(200);
    const r2 = await SELF.fetch(await signedGetUrl(body.content_id as string));
    expect(r2.status).toBe(404);
  });

  it("unauthenticated callers cannot consume a single-use row", async () => {
    const body = await seedWrappedKey({
      single_use: true,
      display_duration_seconds: 10,
    });
    const unauth = await SELF.fetch(
      `http://test/v1/wrapped-keys/${body.content_id}`,
    );
    expect(unauth.status).toBe(401);
    const authenticated = await SELF.fetch(
      await signedGetUrl(body.content_id as string),
    );
    expect(authenticated.status).toBe(200);
  });

  it("rejects replay even if the content id is reinserted", async () => {
    const body = await seedWrappedKey({
      single_use: true,
      display_duration_seconds: 10,
    });
    const signedUrl = await signedGetUrl(body.content_id as string);
    expect((await SELF.fetch(signedUrl)).status).toBe(200);
    const capturedReplay = await postSignedWrappedKey(body, senderSigningKey);
    expect(capturedReplay.status).toBe(409);
    const reinsert = await postSignedWrappedKey(
      { ...body, timestamp_ms: (body.timestamp_ms as number) + 1 },
      senderSigningKey,
    );
    expect(reinsert.status).toBe(201);
    expect((await SELF.fetch(signedUrl)).status).toBe(409);
    expect(
      (await SELF.fetch(await signedGetUrl(body.content_id as string))).status,
    ).toBe(200);
  });

  it("rejects a signature for the wrong recipient without consuming", async () => {
    const otherId = `other-${Math.random().toString(36).slice(2, 8)}`;
    const other = await registerTestUser(SELF, otherId);
    const body = await seedWrappedKey({
      single_use: true,
      display_duration_seconds: 10,
    });
    const wrong = await SELF.fetch(
      await signedGetUrl(
        body.content_id as string,
        otherId,
        otherId,
        other.signingKey,
      ),
    );
    expect(wrong.status).toBe(401);
    expect(
      (await SELF.fetch(await signedGetUrl(body.content_id as string))).status,
    ).toBe(200);
  });

  it("concurrent valid reads return a single-use row at most once", async () => {
    const body = await seedWrappedKey({
      single_use: true,
      display_duration_seconds: 10,
    });
    const urls = await Promise.all([
      signedGetUrl(body.content_id as string),
      signedGetUrl(body.content_id as string),
      signedGetUrl(body.content_id as string),
      signedGetUrl(body.content_id as string),
    ]);
    const responses = await Promise.all(urls.map((url) => SELF.fetch(url)));
    expect(responses.filter((r) => r.status === 200)).toHaveLength(1);
    expect(responses.filter((r) => r.status === 404)).toHaveLength(3);
  });

  it("identity-key CAS blocks single-use consumption by a replaced key", async () => {
    const body = await seedWrappedKey({
      single_use: true,
      display_duration_seconds: 10,
    });
    await testDb
      .prepare("UPDATE users SET ik_ed25519_pub = ? WHERE user_id = ?")
      .bind(base64Encode(new Uint8Array(32).fill(0xee)), recipientId)
      .run();
    const result = await fetchWrappedKeyAuthenticated(
      testDb,
      body.content_id as string,
      recipientId,
      new Uint8Array(32).fill(0xaa),
      recipientPublicKeyB64,
    );
    expect(result.status).toBe("stale_identity");
    const stillPresent = await testDb
      .prepare("SELECT COUNT(*) AS count FROM wrapped_keys WHERE content_id = ?")
      .bind(body.content_id)
      .first<{ count: number }>();
    expect(stillPresent?.count).toBe(1);
  });
});

describe("DELETE /v1/wrapped-keys (burn)", () => {
  let aliceUserId: string;
  let aliceSigningKey: CryptoKey;

  beforeEach(async () => {
    aliceUserId = `alice-${Math.random().toString(36).slice(2, 8)}`;
    const pair = await registerTestUser(SELF, aliceUserId);
    aliceSigningKey = pair.signingKey;
  });

  async function postWrappedKeyForAlice(): Promise<string> {
    const body = validBody({ sender_id: aliceUserId });
    const r = await postSignedWrappedKey(body, aliceSigningKey);
    expect(r.status).toBe(201);
    return body.content_id as string;
  }

  it("scope=single burns exactly the named content_id", async () => {
    const cidA = await postWrappedKeyForAlice();
    const cidB = await postWrappedKeyForAlice();
    const meta = burnMeta();
    const message = canonicalBurnBytes({
      user_id: aliceUserId,
      ...meta,
      scope: "single",
      target: { content_id: cidA },
    });
    const sig = await signEd25519(aliceSigningKey, message);
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "DELETE",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        scope: "single",
        user_id: aliceUserId,
        ...meta,
        target_content_id: cidA,
        burn_signature_b64: sig,
      }),
    });
    expect(res.status).toBe(200);
    const j = (await res.json()) as { scope: string; deleted_count: number };
    expect(j.deleted_count).toBe(1);
    // Confirm cidA is gone, cidB is still there without bypassing the
    // authenticated GET contract merely to inspect test state.
    const rows = await testDb.prepare(
      "SELECT content_id FROM wrapped_keys WHERE content_id IN (?, ?)",
    )
      .bind(cidA, cidB)
      .all<{ content_id: string }>();
    expect(rows.results.map((r) => r.content_id)).toEqual([cidB]);
  });

  it("scope=all wipes only the burning user's rows", async () => {
    await postWrappedKeyForAlice();
    await postWrappedKeyForAlice();
    // A row from a different user — must NOT be touched.
    const otherId = `other-${Math.random().toString(36).slice(2, 8)}`;
    const other = await registerTestUser(SELF, otherId);
    const otherPost = await postSignedWrappedKey(
      validBody({ sender_id: otherId }),
      other.signingKey,
    );
    expect(otherPost.status).toBe(201);
    const meta = burnMeta();
    const message = canonicalBurnBytes({
      user_id: aliceUserId,
      ...meta,
      scope: "all",
    });
    const sig = await signEd25519(aliceSigningKey, message);
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "DELETE",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        scope: "all",
        user_id: aliceUserId,
        ...meta,
        burn_signature_b64: sig,
      }),
    });
    expect(res.status).toBe(200);
    const j = (await res.json()) as { deleted_count: number };
    expect(j.deleted_count).toBe(2);
  });

  it("does not let an old burn delete content created after that burn", async () => {
    await postWrappedKeyForAlice();
    const meta = burnMeta();
    const message = canonicalBurnBytes({
      user_id: aliceUserId,
      ...meta,
      scope: "all",
    });
    const body = {
      scope: "all",
      user_id: aliceUserId,
      ...meta,
      burn_signature_b64: await signEd25519(aliceSigningKey, message),
    };
    const sendBurn = () =>
      SELF.fetch("http://test/v1/wrapped-keys", {
        method: "DELETE",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      });
    expect((await sendBurn()).status).toBe(200);
    const freshContentId = await postWrappedKeyForAlice();

    expect((await sendBurn()).status).toBe(409);
    const row = await testDb
      .prepare("SELECT 1 AS ok FROM wrapped_keys WHERE content_id = ?")
      .bind(freshContentId)
      .first<{ ok: number }>();
    expect(row?.ok).toBe(1);
  });

  it("401s when burn signature is invalid", async () => {
    const cid = await postWrappedKeyForAlice();
    const wrongSig = base64Encode(new Uint8Array(64).fill(0xaa));
    const meta = burnMeta();
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "DELETE",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        scope: "single",
        user_id: aliceUserId,
        ...meta,
        target_content_id: cid,
        burn_signature_b64: wrongSig,
      }),
    });
    expect(res.status).toBe(401);
  });

  it("400s on scope=all with stray target fields", async () => {
    const meta = burnMeta();
    const sig = await signEd25519(
      aliceSigningKey,
      canonicalBurnBytes({ user_id: aliceUserId, ...meta, scope: "all" }),
    );
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "DELETE",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        scope: "all",
        user_id: aliceUserId,
        ...meta,
        target_user_id: "bob",
        burn_signature_b64: sig,
      }),
    });
    expect(res.status).toBe(400);
  });
});

// Avoid unused-export warning since helpers re-imports SELF type indirectly.
