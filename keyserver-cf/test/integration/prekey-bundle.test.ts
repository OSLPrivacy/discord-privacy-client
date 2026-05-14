import { SELF } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import { canonicalReplenishBytes } from "../../src/lib/canonical.js";
import {
  ADMIN_HEADERS,
  base64Encode,
  registerTestUser,
  signEd25519,
} from "./helpers.js";

const STUB_SPK_PUB = base64Encode(new Uint8Array(32).fill(0x55));
const STUB_OPK_PUB = base64Encode(new Uint8Array(32).fill(0x66));

function makeOpk(id: number): { id: number; pub_b64: string } {
  return { id, pub_b64: base64Encode(new Uint8Array(32).fill(id & 0xff)) };
}

describe("POST /v1/prekey-bundle/replenish", () => {
  let userId: string;
  let signingKey: CryptoKey;

  beforeEach(async () => {
    userId = `alice-${Math.random().toString(36).slice(2, 8)}`;
    const pair = await registerTestUser(SELF, userId);
    signingKey = pair.signingKey;
  });

  it("200s on a valid SPK + OPK batch with correct signature", async () => {
    const spk = {
      pub_b64: STUB_SPK_PUB,
      signature_b64: base64Encode(new Uint8Array(64).fill(0x77)),
      rotated_at: new Date().toISOString(),
    };
    const opks = [makeOpk(1), makeOpk(2), makeOpk(3)];
    const message = canonicalReplenishBytes({ user_id: userId, spk, opks });
    const batch_signature_b64 = await signEd25519(signingKey, message);
    const res = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        user_id: userId,
        spk,
        opks,
        batch_signature_b64,
      }),
    });
    expect(res.status).toBe(200);
    const j = (await res.json()) as { user_id: string; opks_added: number };
    expect(j.user_id).toBe(userId);
    expect(j.opks_added).toBe(3);
  });

  it("401s on wrong signature", async () => {
    const opks = [makeOpk(10)];
    const wrong = base64Encode(new Uint8Array(64).fill(0xaa));
    const res = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        user_id: userId,
        opks,
        batch_signature_b64: wrong,
      }),
    });
    expect(res.status).toBe(401);
  });

  it("404s when user not registered", async () => {
    const opks = [makeOpk(20)];
    const message = canonicalReplenishBytes({
      user_id: "ghost",
      spk: null,
      opks,
    });
    const sig = await signEd25519(signingKey, message);
    const res = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        user_id: "ghost",
        opks,
        batch_signature_b64: sig,
      }),
    });
    expect(res.status).toBe(404);
  });

  it("409s on duplicate opk_id under same user", async () => {
    const opks = [makeOpk(42)];
    const m1 = canonicalReplenishBytes({ user_id: userId, spk: null, opks });
    const s1 = await signEd25519(signingKey, m1);
    const r1 = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        user_id: userId,
        opks,
        batch_signature_b64: s1,
      }),
    });
    expect(r1.status).toBe(200);
    // Re-submit the SAME opk.id; expect 409.
    const m2 = canonicalReplenishBytes({ user_id: userId, spk: null, opks });
    const s2 = await signEd25519(signingKey, m2);
    const r2 = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        user_id: userId,
        opks,
        batch_signature_b64: s2,
      }),
    });
    expect(r2.status).toBe(409);
  });
});

describe("GET /v1/prekey-bundle/:user_id (atomic pop)", () => {
  let userId: string;
  let signingKey: CryptoKey;

  beforeEach(async () => {
    userId = `alice-${Math.random().toString(36).slice(2, 8)}`;
    const pair = await registerTestUser(SELF, userId);
    signingKey = pair.signingKey;
    // Seed an SPK + 3 OPKs.
    const spk = {
      pub_b64: STUB_SPK_PUB,
      signature_b64: base64Encode(new Uint8Array(64).fill(0x88)),
      rotated_at: new Date().toISOString(),
    };
    const opks = [makeOpk(1), makeOpk(2), makeOpk(3)];
    const message = canonicalReplenishBytes({ user_id: userId, spk, opks });
    const batch_signature_b64 = await signEd25519(signingKey, message);
    const r = await SELF.fetch("http://test/v1/prekey-bundle/replenish", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        user_id: userId,
        spk,
        opks,
        batch_signature_b64,
      }),
    });
    expect(r.status).toBe(200);
  });

  it("returns the bundle with one OPK and decreasing remaining_opk_count", async () => {
    const r1 = await SELF.fetch(`http://test/v1/prekey-bundle/${userId}`);
    expect(r1.status).toBe(200);
    const b1 = (await r1.json()) as {
      opk: { id: number; pub_b64: string } | null;
      remaining_opk_count: number;
    };
    expect(b1.opk).not.toBeNull();
    expect(b1.remaining_opk_count).toBe(2);

    const r2 = await SELF.fetch(`http://test/v1/prekey-bundle/${userId}`);
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
    await SELF.fetch(`http://test/v1/prekey-bundle/${userId}`);
    await SELF.fetch(`http://test/v1/prekey-bundle/${userId}`);
    await SELF.fetch(`http://test/v1/prekey-bundle/${userId}`);
    const res = await SELF.fetch(`http://test/v1/prekey-bundle/${userId}`);
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
    const res = await SELF.fetch(`http://test/v1/prekey-bundle/never-replenished`);
    expect(res.status).toBe(404);
  });

  it("concurrent pops do not return the same OPK id", async () => {
    // Critical regression test for db.batch atomicity. Fire 3 pops
    // in parallel and verify each gets a distinct opk.id.
    const [r1, r2, r3] = await Promise.all([
      SELF.fetch(`http://test/v1/prekey-bundle/${userId}`),
      SELF.fetch(`http://test/v1/prekey-bundle/${userId}`),
      SELF.fetch(`http://test/v1/prekey-bundle/${userId}`),
    ]);
    const bundles = (await Promise.all([r1.json(), r2.json(), r3.json()])) as {
      opk: { id: number } | null;
    }[];
    const ids = bundles.map((b) => b.opk?.id).filter((x): x is number => x !== undefined);
    const distinct = new Set(ids);
    expect(distinct.size).toBe(ids.length);
  });
});
