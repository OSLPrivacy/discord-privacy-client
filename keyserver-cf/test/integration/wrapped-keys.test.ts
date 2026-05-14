import { SELF } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import { canonicalBurnBytes } from "../../src/lib/canonical.js";
import {
  ADMIN_HEADERS,
  base64Encode,
  registerTestUser,
  signEd25519,
  TEST_ADMIN_TOKEN,
} from "./helpers.js";

const STUB_SHARE_BLOB = base64Encode(new Uint8Array([1, 2, 3, 4]));
const FUTURE_ISO = () => new Date(Date.now() + 5 * 60_000).toISOString();
const PAST_ISO = () => new Date(Date.now() - 1000).toISOString();

function uniqueContentId(prefix = "msg"): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
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
    ...overrides,
  };
}

describe("POST /v1/wrapped-keys", () => {
  it("401s without admin token", async () => {
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(validBody()),
    });
    expect(res.status).toBe(401);
  });

  it("201s on valid insert", async () => {
    const body = validBody();
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(body),
    });
    expect(res.status).toBe(201);
    expect(await res.json()).toEqual({ content_id: body.content_id });
  });

  it("409s on duplicate content_id", async () => {
    const body = validBody();
    await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(body),
    });
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(body),
    });
    expect(res.status).toBe(409);
  });

  it("400s missing display_duration_seconds when single_use=true", async () => {
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(validBody({ single_use: true })),
    });
    expect(res.status).toBe(400);
  });

  it("400s display_duration_seconds present when single_use=false", async () => {
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(validBody({ display_duration_seconds: 5 })),
    });
    expect(res.status).toBe(400);
  });

  it("400s system_message_kind on non-system content_type", async () => {
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(validBody({ system_message_kind: "burn-alert" })),
    });
    expect(res.status).toBe(400);
  });

  it("201s system content_type with allowed kind", async () => {
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(
        validBody({ content_type: "system", system_message_kind: "burn-alert" }),
      ),
    });
    expect(res.status).toBe(201);
  });
});

describe("GET /v1/wrapped-keys/:content_id", () => {
  it("404s for unknown content_id", async () => {
    const res = await SELF.fetch("http://test/v1/wrapped-keys/unknown-id");
    expect(res.status).toBe(404);
  });

  it("returns the row for a fresh insert (public, no auth)", async () => {
    const body = validBody();
    await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(body),
    });
    const res = await SELF.fetch(`http://test/v1/wrapped-keys/${body.content_id}`);
    expect(res.status).toBe(200);
    const j = (await res.json()) as Record<string, unknown>;
    expect(j.content_id).toBe(body.content_id);
    expect(j.sender_id).toBe(body.sender_id);
    expect(j.wrapped_share_blob).toBe(body.wrapped_share_blob);
    expect(j.single_use).toBe(false);
  });

  it("410s a past-expiry row and tombstones it", async () => {
    const body = validBody({ expires_at: PAST_ISO() });
    await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(body),
    });
    const r1 = await SELF.fetch(`http://test/v1/wrapped-keys/${body.content_id}`);
    expect(r1.status).toBe(410);
    // Subsequent fetch: row tombstoned → 404
    const r2 = await SELF.fetch(`http://test/v1/wrapped-keys/${body.content_id}`);
    expect(r2.status).toBe(404);
  });

  it("single_use row is consumed on first read", async () => {
    const body = validBody({ single_use: true, display_duration_seconds: 10 });
    await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(body),
    });
    const r1 = await SELF.fetch(`http://test/v1/wrapped-keys/${body.content_id}`);
    expect(r1.status).toBe(200);
    const r2 = await SELF.fetch(`http://test/v1/wrapped-keys/${body.content_id}`);
    expect(r2.status).toBe(404);
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
    const r = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(body),
    });
    expect(r.status).toBe(201);
    return body.content_id as string;
  }

  it("scope=single burns exactly the named content_id", async () => {
    const cidA = await postWrappedKeyForAlice();
    const cidB = await postWrappedKeyForAlice();
    const message = canonicalBurnBytes({
      user_id: aliceUserId,
      scope: "single",
      target: { content_id: cidA },
    });
    const sig = await signEd25519(aliceSigningKey, message);
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "DELETE",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        scope: "single",
        user_id: aliceUserId,
        target_content_id: cidA,
        burn_signature_b64: sig,
      }),
    });
    expect(res.status).toBe(200);
    const j = (await res.json()) as { scope: string; deleted_count: number };
    expect(j.deleted_count).toBe(1);
    // Confirm cidA is gone, cidB is still there.
    expect((await SELF.fetch(`http://test/v1/wrapped-keys/${cidA}`)).status).toBe(404);
    expect((await SELF.fetch(`http://test/v1/wrapped-keys/${cidB}`)).status).toBe(200);
  });

  it("scope=all wipes only the burning user's rows", async () => {
    await postWrappedKeyForAlice();
    await postWrappedKeyForAlice();
    // A row from a different user — must NOT be touched.
    const other = await registerTestUser(SELF, "other-sender");
    void other;
    await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(validBody({ sender_id: "other-sender" })),
    });
    const message = canonicalBurnBytes({
      user_id: aliceUserId,
      scope: "all",
    });
    const sig = await signEd25519(aliceSigningKey, message);
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "DELETE",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        scope: "all",
        user_id: aliceUserId,
        burn_signature_b64: sig,
      }),
    });
    expect(res.status).toBe(200);
    const j = (await res.json()) as { deleted_count: number };
    expect(j.deleted_count).toBe(2);
  });

  it("401s when burn signature is invalid", async () => {
    const cid = await postWrappedKeyForAlice();
    const wrongSig = base64Encode(new Uint8Array(64).fill(0xaa));
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "DELETE",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        scope: "single",
        user_id: aliceUserId,
        target_content_id: cid,
        burn_signature_b64: wrongSig,
      }),
    });
    expect(res.status).toBe(401);
  });

  it("400s on scope=all with stray target fields", async () => {
    const sig = await signEd25519(
      aliceSigningKey,
      canonicalBurnBytes({ user_id: aliceUserId, scope: "all" }),
    );
    const res = await SELF.fetch("http://test/v1/wrapped-keys", {
      method: "DELETE",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        scope: "all",
        user_id: aliceUserId,
        target_user_id: "bob",
        burn_signature_b64: sig,
      }),
    });
    expect(res.status).toBe(400);
  });
});

// Avoid unused-export warning since helpers re-imports SELF type indirectly.
void TEST_ADMIN_TOKEN;
