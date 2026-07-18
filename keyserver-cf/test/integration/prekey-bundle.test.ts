import { SELF, env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import {
  canonicalPrekeyBundleGetBytes,
  canonicalReplenishBytes,
} from "../../src/lib/canonical.js";
import { popPrekeyBundleAuthenticated } from "../../src/lib/db.js";
import {
  ADMIN_HEADERS,
  base64Encode,
  registerTestUser,
  signEd25519,
} from "./helpers.js";

const STUB_SPK_PUB = base64Encode(new Uint8Array(32).fill(0x55));
const STUB_OPK_PUB = base64Encode(new Uint8Array(32).fill(0x66));
let signedGetSequence = 0;
let replenishSequence = 1;
const testDb = (env as unknown as { DB: D1Database }).DB;

function makeOpk(id: number): { id: number; pub_b64: string } {
  return { id, pub_b64: base64Encode(new Uint8Array(32).fill(id & 0xff)) };
}

function requestMeta(timestamp_ms = Date.now()): {
  timestamp_ms: number;
  request_id: string;
} {
  const bytes = new Uint8Array(32).fill((replenishSequence++ % 250) + 1);
  const request_id = base64Encode(bytes)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
  return { timestamp_ms, request_id };
}

async function makeSpk(signingKey: CryptoKey, fill = 0x55, when = new Date()): Promise<{
  pub_b64: string;
  signature_b64: string;
  rotated_at: string;
}> {
  const publicKey = new Uint8Array(32).fill(fill);
  return {
    pub_b64: base64Encode(publicKey),
    signature_b64: await signEd25519(signingKey, publicKey),
    rotated_at: when.toISOString(),
  };
}

async function signedReplenishBody(
  user_id: string,
  signingKey: CryptoKey,
  spk: Awaited<ReturnType<typeof makeSpk>> | null,
  opks: { id: number; pub_b64: string }[],
  meta = requestMeta(),
): Promise<Record<string, unknown>> {
  const message = canonicalReplenishBytes({ user_id, ...meta, spk, opks });
  return {
    user_id,
    ...meta,
    spk,
    opks,
    batch_signature_b64: await signEd25519(signingKey, message),
  };
}

describe("POST /v1/prekey-bundle/replenish", () => {
  let userId: string;
  let signingKey: CryptoKey;

  beforeEach(async () => {
    userId = `alice-${Math.random().toString(36).slice(2, 8)}`;
    const pair = await registerTestUser(SELF, userId);
    signingKey = pair.signingKey;
  });

  async function postReplenish(body: Record<string, unknown>): Promise<Response> {
    return SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  }

  it("200s on a valid signed batch without a shared bearer", async () => {
    const spk = await makeSpk(signingKey);
    const opks = [makeOpk(1), makeOpk(2), makeOpk(3)];
    const body = await signedReplenishBody(userId, signingKey, spk, opks);
    const res = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    expect(res.status).toBe(200);
    const j = (await res.json()) as { user_id: string; opks_added: number };
    expect(j.user_id).toBe(userId);
    expect(j.opks_added).toBe(3);
  });

  it("401s on wrong signature", async () => {
    const opks = [makeOpk(10)];
    const wrong = base64Encode(new Uint8Array(64).fill(0xaa));
    const meta = requestMeta();
    const res = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        user_id: userId,
        ...meta,
        opks,
        batch_signature_b64: wrong,
      }),
    });
    expect(res.status).toBe(401);
  });

  it("404s when user not registered", async () => {
    const opks = [makeOpk(20)];
    const meta = requestMeta();
    const message = canonicalReplenishBytes({
      user_id: "ghost",
      ...meta,
      spk: null,
      opks,
    });
    const sig = await signEd25519(signingKey, message);
    const res = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        user_id: "ghost",
        ...meta,
        opks,
        batch_signature_b64: sig,
      }),
    });
    expect(res.status).toBe(404);
  });

  it("409s on duplicate opk_id under same user", async () => {
    const opks = [makeOpk(42)];
    const body1 = await signedReplenishBody(
      userId,
      signingKey,
      await makeSpk(signingKey),
      opks,
    );
    const r1 = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(body1),
    });
    expect(r1.status).toBe(200);
    // Re-submit the SAME opk.id; expect 409.
    const body2 = await signedReplenishBody(userId, signingKey, null, opks);
    const r2 = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify(body2),
    });
    expect(r2.status).toBe(409);
  });

  it("rejects an OPK id above u32 before signature verification", async () => {
    const res = await postReplenish({
      user_id: userId,
      ...requestMeta(),
      opks: [{ id: 0x1_0000_0000, pub_b64: STUB_OPK_PUB }],
      batch_signature_b64: base64Encode(new Uint8Array(64)),
    });
    expect(res.status).toBe(400);
    expect(await res.json()).toEqual({ error: "opk.id must be u32" });
  });

  it("does not reinsert an OPK when a consumed replenish request is replayed", async () => {
    const body = await signedReplenishBody(
      userId,
      signingKey,
      await makeSpk(signingKey),
      [makeOpk(700)],
    );
    expect((await postReplenish(body)).status).toBe(200);
    await testDb.prepare("DELETE FROM opk_pool WHERE user_id = ? AND opk_id = 700")
      .bind(userId)
      .run();

    expect((await postReplenish(body)).status).toBe(409);
    const row = await testDb
      .prepare("SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ? AND opk_id = 700")
      .bind(userId)
      .first<{ count: number }>();
    expect(row?.count).toBe(0);
  });

  it("does not roll an SPK back when an older replenish request is replayed", async () => {
    const oldSpk = await makeSpk(signingKey, 0x31, new Date(Date.now() - 2_000));
    const newSpk = await makeSpk(signingKey, 0x32, new Date());
    const oldBody = await signedReplenishBody(userId, signingKey, oldSpk, []);
    const newBody = await signedReplenishBody(userId, signingKey, newSpk, []);
    expect((await postReplenish(oldBody)).status).toBe(200);
    expect((await postReplenish(newBody)).status).toBe(200);

    const rollbackBody = await signedReplenishBody(
      userId,
      signingKey,
      oldSpk,
      [makeOpk(701)],
    );
    expect((await postReplenish(rollbackBody)).status).toBe(409);
    const row = await testDb
      .prepare("SELECT spk_pub FROM prekey_bundles WHERE user_id = ?")
      .bind(userId)
      .first<{ spk_pub: string }>();
    expect(row?.spk_pub).toBe(newSpk.pub_b64);
    const rolledBackOpk = await testDb
      .prepare("SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ? AND opk_id = 701")
      .bind(userId)
      .first<{ count: number }>();
    expect(rolledBackOpk?.count).toBe(0);
  });

  it("accepts the exact current SPK while adding fresh OPKs", async () => {
    const spk = await makeSpk(signingKey);
    expect(
      (await postReplenish(await signedReplenishBody(userId, signingKey, spk, [])))
        .status,
    ).toBe(200);

    const response = await postReplenish(
      await signedReplenishBody(userId, signingKey, spk, [makeOpk(702)]),
    );
    expect(response.status).toBe(200);
    const stored = await testDb
      .prepare("SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ? AND opk_id = 702")
      .bind(userId)
      .first<{ count: number }>();
    expect(stored?.count).toBe(1);
  });

  it("rejects OPK-only replenish before the identity has an SPK", async () => {
    const response = await postReplenish(
      await signedReplenishBody(userId, signingKey, null, [makeOpk(703)]),
    );
    expect(response.status).toBe(409);
    expect(await response.json()).toEqual({
      error: "upload an SPK before an OPK-only replenish",
    });
    const stored = await testDb
      .prepare("SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ?")
      .bind(userId)
      .first<{ count: number }>();
    expect(stored?.count).toBe(0);
  });

  it("enforces the 200-key pool cap atomically at the D1 boundary", async () => {
    const spk = await makeSpk(signingKey);
    expect(
      (await postReplenish(await signedReplenishBody(userId, signingKey, spk, [])))
        .status,
    ).toBe(200);
    await testDb
      .prepare(
        `WITH RECURSIVE seq(n) AS (
           SELECT 1 UNION ALL SELECT n + 1 FROM seq WHERE n < 200
         )
         INSERT INTO opk_pool (user_id, opk_id, opk_pub)
         SELECT ?, n + 10000, ? FROM seq`,
      )
      .bind(userId, STUB_OPK_PUB)
      .run();

    const body = await signedReplenishBody(
      userId,
      signingKey,
      null,
      [makeOpk(704)],
    );
    const blocked = await postReplenish(body);
    expect(blocked.status).toBe(429);

    await testDb
      .prepare("DELETE FROM opk_pool WHERE user_id = ? AND opk_id = 10001")
      .bind(userId)
      .run();
    // A quota failure must roll back the replay receipt as well as the insert.
    expect((await postReplenish(body)).status).toBe(200);
    const stored = await testDb
      .prepare("SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ?")
      .bind(userId)
      .first<{ count: number }>();
    expect(stored?.count).toBe(200);
  });
});

describe("GET /v1/prekey-bundle/:user_id (atomic pop)", () => {
  let userId: string;
  let signingKey: CryptoKey;
  let requesterId: string;
  let requesterSigningKey: CryptoKey;
  let requesterPublicKeyB64: string;

  beforeEach(async () => {
    userId = `alice-${Math.random().toString(36).slice(2, 8)}`;
    const pair = await registerTestUser(SELF, userId);
    signingKey = pair.signingKey;
    requesterId = `requester-${Math.random().toString(36).slice(2, 8)}`;
    const requester = await registerTestUser(SELF, requesterId);
    requesterSigningKey = requester.signingKey;
    requesterPublicKeyB64 = requester.publicKeyB64;
    // Seed an SPK + 3 OPKs.
    const spk = await makeSpk(signingKey);
    const opks = [makeOpk(1), makeOpk(2), makeOpk(3)];
    const body = await signedReplenishBody(userId, signingKey, spk, opks);
    const r = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: {
        ...ADMIN_HEADERS,
        "x-forwarded-for": `198.51.100.${(signedGetSequence++ % 250) + 1}`,
      },
      body: JSON.stringify(body),
    });
    expect(r.status).toBe(200);
  });

  async function signedGetUrl(
    recipientId = userId,
    signedRecipientId = recipientId,
    timestampMs = Date.now() + signedGetSequence++,
    key = requesterSigningKey,
  ): Promise<string> {
    const message = canonicalPrekeyBundleGetBytes({
      requester_id: requesterId,
      recipient_id: signedRecipientId,
      timestamp_ms: timestampMs,
    });
    const sig = await signEd25519(key, message);
    const q = new URLSearchParams({
      requester_id: requesterId,
      recipient_id: signedRecipientId,
      ts: String(timestampMs),
      sig,
    });
    return `http://test/v1/prekey-bundle/${encodeURIComponent(recipientId)}?${q}`;
  }

  it("returns the bundle with one OPK and decreasing remaining_opk_count", async () => {
    const r1 = await SELF.fetch(await signedGetUrl());
    expect(r1.status).toBe(200);
    const b1 = (await r1.json()) as {
      opk: { id: number; pub_b64: string } | null;
      remaining_opk_count: number;
    };
    expect(b1.opk).not.toBeNull();
    expect(b1.remaining_opk_count).toBe(2);

    const r2 = await SELF.fetch(await signedGetUrl());
    expect(r2.status).toBe(200);
    const b2 = (await r2.json()) as {
      opk: { id: number; pub_b64: string } | null;
      remaining_opk_count: number;
    };
    expect(b2.opk).not.toBeNull();
    expect(b2.remaining_opk_count).toBe(1);
    // OPKs are popped in id-ascending order, so the second pop must
    // be a strictly larger id than the first.
    expect(b2.opk!.id).toBeGreaterThan(b1.opk!.id);
  });

  it("returns opk=null after pool exhausted (OPK-exhaustion fallback)", async () => {
    // Drain all 3.
    await SELF.fetch(await signedGetUrl());
    await SELF.fetch(await signedGetUrl());
    await SELF.fetch(await signedGetUrl());
    const res = await SELF.fetch(await signedGetUrl());
    expect(res.status).toBe(200);
    const j = (await res.json()) as {
      opk: unknown;
      remaining_opk_count: number;
      spk_pub: string;
    };
    expect(j.opk).toBeNull();
    expect(j.remaining_opk_count).toBe(0);
    expect(j.spk_pub).toBe(STUB_SPK_PUB);
  });

  it("404s for a user that's never replenished", async () => {
    const res = await SELF.fetch(
      await signedGetUrl("never-replenished", "never-replenished"),
    );
    expect(res.status).toBe(404);
  });

  it("unauthenticated callers cannot consume an OPK", async () => {
    const unauth = await SELF.fetch(`http://test/v1/prekey-bundle/${userId}`);
    expect(unauth.status).toBe(401);
    const authenticated = await SELF.fetch(await signedGetUrl());
    expect(authenticated.status).toBe(200);
    const bundle = (await authenticated.json()) as {
      opk: { id: number };
      remaining_opk_count: number;
    };
    expect(bundle.opk.id).toBe(1);
    expect(bundle.remaining_opk_count).toBe(2);
  });

  it("rejects replay without consuming another OPK", async () => {
    const url = await signedGetUrl();
    expect((await SELF.fetch(url)).status).toBe(200);
    expect((await SELF.fetch(url)).status).toBe(409);
    const remaining = await testDb.prepare(
      "SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ?",
    )
      .bind(userId)
      .first<{ count: number }>();
    expect(remaining?.count).toBe(2);
  });

  it("rejects a signature bound to the wrong recipient", async () => {
    const res = await SELF.fetch(await signedGetUrl(userId, "someone-else"));
    expect(res.status).toBe(401);
    const remaining = await testDb.prepare(
      "SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ?",
    )
      .bind(userId)
      .first<{ count: number }>();
    expect(remaining?.count).toBe(3);
  });

  it("rejects stale signed requests without consuming", async () => {
    const res = await SELF.fetch(
      await signedGetUrl(userId, userId, Date.now() - 6 * 60_000),
    );
    expect(res.status).toBe(401);
  });

  it("identity-key CAS blocks a pop authorized by a replaced key", async () => {
    const message = canonicalPrekeyBundleGetBytes({
      requester_id: requesterId,
      recipient_id: userId,
      timestamp_ms: Date.now(),
    });
    const digest = new Uint8Array(
      await crypto.subtle.digest("SHA-256", message),
    );
    await testDb
      .prepare("UPDATE users SET ik_ed25519_pub = ? WHERE user_id = ?")
      .bind(base64Encode(new Uint8Array(32).fill(0xee)), requesterId)
      .run();
    const result = await popPrekeyBundleAuthenticated(
      testDb,
      userId,
      requesterId,
      digest,
      requesterPublicKeyB64,
    );
    expect(result.status).toBe("stale_identity");
    const remaining = await testDb
      .prepare("SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ?")
      .bind(userId)
      .first<{ count: number }>();
    expect(remaining?.count).toBe(3);
  });

  it("concurrent pops do not return the same OPK id", async () => {
    // Critical regression test for db.batch atomicity. Fire 3 pops
    // in parallel and verify each gets a distinct opk.id.
    const [url1, url2, url3] = await Promise.all([
      signedGetUrl(),
      signedGetUrl(),
      signedGetUrl(),
    ]);
    const [r1, r2, r3] = await Promise.all([
      SELF.fetch(url1),
      SELF.fetch(url2),
      SELF.fetch(url3),
    ]);
    const bundles = (await Promise.all([r1.json(), r2.json(), r3.json()])) as {
      opk: { id: number } | null;
    }[];
    const ids = bundles.map((b) => b.opk?.id).filter((x): x is number => x !== undefined);
    const distinct = new Set(ids);
    expect(distinct.size).toBe(ids.length);
  });

  it("concurrent copies of one valid request consume at most once", async () => {
    const url = await signedGetUrl();
    const responses = await Promise.all([
      SELF.fetch(url),
      SELF.fetch(url),
      SELF.fetch(url),
      SELF.fetch(url),
    ]);
    expect(responses.filter((r) => r.status === 200)).toHaveLength(1);
    expect(responses.filter((r) => r.status === 409)).toHaveLength(3);
    const remaining = await testDb.prepare(
      "SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ?",
    )
      .bind(userId)
      .first<{ count: number }>();
    expect(remaining?.count).toBe(2);
  });
});
