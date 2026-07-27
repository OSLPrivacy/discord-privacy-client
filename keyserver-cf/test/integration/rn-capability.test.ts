/// Signed protocol-capability advertisement (migration 0026).
///
/// The property under test is not "the bitmap round-trips". It is that
/// **the bitmap cannot be added, removed, altered or lowered by anyone
/// who does not hold the identity's Ed25519 secret**, and that every
/// failure mode reads as "no OSL-RN capability" rather than as
/// "capable" — see `crates/osl-ratchet-next/src/negotiate.rs`, layer L1.

import { env, SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  buildRegMsg,
  buildRotMsg,
  parseRnCapabilities,
  RN_CAP_MAX,
  RN_CAP_WIRE_RN,
} from "../../src/lib/signed-request.js";
import {
  generateEd25519Pair,
  signedRegisterBody,
  signEd25519,
  STUB_MLKEM_PUB_B64,
  STUB_RATCHET_PUB_B64,
  STUB_X25519_PUB_B64,
} from "./helpers.js";

let n = 0;
const uid = () => `cap-${Date.now()}-${n++}`;

async function post(body: unknown) {
  return SELF.fetch("http://test/v1/register", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

/// A register body that advertises `caps`, signed over the EXTENDED
/// REG_MSG (the one that includes the bitmap).
async function signedRegisterBodyWithCaps(
  userId: string,
  pair: { publicKeyB64: string; signingKey: CryptoKey },
  caps: number,
): Promise<Record<string, unknown>> {
  const fields = {
    user_id: userId,
    ik_x25519_pub: STUB_X25519_PUB_B64,
    ik_ed25519_pub: pair.publicKeyB64,
    ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
    ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
    rn_capabilities: caps,
  };
  const registration_sig = await signEd25519(pair.signingKey, buildRegMsg(fields));
  return { ...fields, registration_sig };
}

async function pubkeys(userId: string): Promise<Record<string, unknown>> {
  const res = await SELF.fetch(`http://test/v1/pubkeys/${userId}`);
  expect(res.status).toBe(200);
  return (await res.json()) as Record<string, unknown>;
}

describe("rn_capabilities — field validation", () => {
  it("absent means no capability, and the legacy REG_MSG still verifies", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    // `signedRegisterBody` is the legacy helper: it signs the
    // pre-capability message and sends no bitmap.
    const body = await signedRegisterBody(userId, pair);
    expect((await post(body)).status).toBe(201);
    const j = await pubkeys(userId);
    expect(j.rn_capabilities).toBe(0);
    // The signature is mandatory for the bundle even when no
    // capability is advertised.
    expect(j.registration_sig).toBe(body.registration_sig);
    expect(j.ik_x25519_signature).toBeUndefined();
  });

  it("refuses a malformed bitmap instead of coercing it to 0", async () => {
    for (const bad of ["1", 1.5, -1, RN_CAP_MAX + 1, Number.NaN, true, {}, []]) {
      const pair = await generateEd25519Pair();
      const body = await signedRegisterBodyWithCaps(uid(), pair, RN_CAP_WIRE_RN);
      body.rn_capabilities = bad;
      const res = await post(body);
      expect(res.status, `rn_capabilities=${JSON.stringify(bad)}`).toBe(400);
    }
  });

  it("parseRnCapabilities distinguishes absent from malformed", () => {
    expect(parseRnCapabilities(undefined)).toEqual({ present: false, value: 0 });
    expect(parseRnCapabilities(null)).toEqual({ present: false, value: 0 });
    expect(parseRnCapabilities(0)).toEqual({ present: true, value: 0 });
    expect(parseRnCapabilities(RN_CAP_WIRE_RN)).toEqual({
      present: true,
      value: RN_CAP_WIRE_RN,
    });
    expect(parseRnCapabilities(RN_CAP_MAX)).toEqual({
      present: true,
      value: RN_CAP_MAX,
    });
    // Malformed is `null`, NOT a zero-valued capability.
    expect(parseRnCapabilities("1")).toBeNull();
    expect(parseRnCapabilities(-1)).toBeNull();
    expect(parseRnCapabilities(RN_CAP_MAX + 1)).toBeNull();
    expect(parseRnCapabilities(1.5)).toBeNull();
  });

  it("stores an unknown future bit verbatim rather than rejecting it", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    const future = 1 << 9;
    expect((await post(await signedRegisterBodyWithCaps(userId, pair, future))).status)
      .toBe(201);
    expect((await pubkeys(userId)).rn_capabilities).toBe(future);
  });
});

describe("rn_capabilities — the signature binding", () => {
  it("registers and serves a verifiable advertisement", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    const res = await post(
      await signedRegisterBodyWithCaps(userId, pair, RN_CAP_WIRE_RN),
    );
    expect(res.status).toBe(201);
    expect((await res.json() as Record<string, unknown>).rn_capabilities).toBe(
      RN_CAP_WIRE_RN,
    );

    const j = await pubkeys(userId);
    expect(j.rn_capabilities).toBe(RN_CAP_WIRE_RN);
    expect(typeof j.registration_sig).toBe("string");

    // A reader reconstructs REG_MSG from the served fields and
    // verifies. This is the whole point: the reader does not have to
    // trust the key server's report of the bitmap.
    const rebuilt = buildRegMsg({
      user_id: j.user_id as string,
      ik_x25519_pub: j.ik_x25519_pub as string,
      ik_ed25519_pub: j.ik_ed25519_pub as string,
      ik_mlkem768_pub: j.ik_mlkem768_pub as string,
      ik_ratchet_initial_pub: j.ik_ratchet_initial_pub as string | null,
      rn_capabilities: j.rn_capabilities as number,
    });
    const ok = await crypto.subtle.verify(
      { name: "Ed25519" },
      await crypto.subtle.importKey(
        "raw",
        pair.publicKey,
        { name: "Ed25519" },
        false,
        ["verify"],
      ),
      Uint8Array.from(atob(j.registration_sig as string), (c) => c.charCodeAt(0)),
      rebuilt,
    );
    expect(ok).toBe(true);
  });

  it("refreshes a nonempty advertisement across production register and pubkeys", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    const legacy = await signedRegisterBody(userId, pair);

    expect((await post(legacy)).status).toBe(201);
    const before = await pubkeys(userId);
    expect(before.rn_capabilities).toBe(0);
    expect(before.registration_sig).toBe(legacy.registration_sig);

    // This must be a real positive, not a zero-valued round trip.
    expect(RN_CAP_WIRE_RN).toBeGreaterThan(0);
    const advertised = await signedRegisterBodyWithCaps(
      userId,
      pair,
      RN_CAP_WIRE_RN,
    );
    const register = await post(advertised);
    expect(register.status).toBe(200);
    expect(await register.json()).toMatchObject({
      status: "capabilities_raised",
      rn_capabilities: RN_CAP_WIRE_RN,
    });

    // Fetch through the public production route. A lost bitmap, a stale
    // zero record, or the legacy signature must each fail this boundary.
    const served = await pubkeys(userId);
    expect(served.rn_capabilities).toBe(RN_CAP_WIRE_RN);
    expect(served.registration_sig).toBe(advertised.registration_sig);
    expect(served.registration_sig).not.toBe(before.registration_sig);

    const rebuilt = buildRegMsg({
      user_id: served.user_id as string,
      ik_x25519_pub: served.ik_x25519_pub as string,
      ik_ed25519_pub: served.ik_ed25519_pub as string,
      ik_mlkem768_pub: served.ik_mlkem768_pub as string,
      ik_ratchet_initial_pub: served.ik_ratchet_initial_pub as string | null,
      rn_capabilities: served.rn_capabilities as number,
    });
    expect(
      await crypto.subtle.verify(
        { name: "Ed25519" },
        await crypto.subtle.importKey(
          "raw",
          pair.publicKey,
          { name: "Ed25519" },
          false,
          ["verify"],
        ),
        Uint8Array.from(
          atob(served.registration_sig as string),
          (c) => c.charCodeAt(0),
        ),
        rebuilt,
      ),
    ).toBe(true);
  });

  it("a reader that lowers the served bitmap fails verification", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    await post(await signedRegisterBodyWithCaps(userId, pair, RN_CAP_WIRE_RN));
    const j = await pubkeys(userId);
    const key = await crypto.subtle.importKey(
      "raw",
      pair.publicKey,
      { name: "Ed25519" },
      false,
      ["verify"],
    );
    const sig = Uint8Array.from(atob(j.registration_sig as string), (c) =>
      c.charCodeAt(0),
    );
    const base = {
      user_id: j.user_id as string,
      ik_x25519_pub: j.ik_x25519_pub as string,
      ik_ed25519_pub: j.ik_ed25519_pub as string,
      ik_mlkem768_pub: j.ik_mlkem768_pub as string,
      ik_ratchet_initial_pub: j.ik_ratchet_initial_pub as string | null,
    };
    // Lowered to zero, and stripped entirely (the legacy form). Both
    // must fail, so a read-path attacker cannot make a capable peer
    // look non-capable without being detected.
    for (const tampered of [{ ...base, rn_capabilities: 0 }, base]) {
      const ok = await crypto.subtle.verify(
        { name: "Ed25519" },
        key,
        sig,
        buildRegMsg(tampered),
      );
      expect(ok).toBe(false);
    }
  });

  it("stripping the bitmap from the REQUEST is refused, not applied", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    const body = await signedRegisterBodyWithCaps(userId, pair, RN_CAP_WIRE_RN);
    delete body.rn_capabilities;
    // The server reconstructs the legacy REG_MSG; the signature covers
    // the extended one.
    const res = await post(body);
    expect(res.status).toBe(400);
    // And nothing was written.
    expect((await SELF.fetch(`http://test/v1/pubkeys/${userId}`)).status).toBe(404);
  });

  it("altering the bitmap in the REQUEST is refused", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    const body = await signedRegisterBodyWithCaps(userId, pair, RN_CAP_WIRE_RN);
    body.rn_capabilities = RN_CAP_WIRE_RN | 2;
    expect((await post(body)).status).toBe(400);
    expect((await SELF.fetch(`http://test/v1/pubkeys/${userId}`)).status).toBe(404);
  });

  it("injecting a bitmap onto a legacy-signed body is refused", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    const body = await signedRegisterBody(userId, pair);
    body.rn_capabilities = RN_CAP_WIRE_RN;
    expect((await post(body)).status).toBe(400);
    expect((await SELF.fetch(`http://test/v1/pubkeys/${userId}`)).status).toBe(404);
  });
});

describe("rn_capabilities — monotonicity", () => {
  it("Case B raises the bitmap, then is write-free and replay-inert", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    expect((await post(await signedRegisterBody(userId, pair))).status).toBe(201);
    expect((await pubkeys(userId)).rn_capabilities).toBe(0);

    const raise = await signedRegisterBodyWithCaps(userId, pair, RN_CAP_WIRE_RN);
    const first = await post(raise);
    expect(first.status).toBe(200);
    expect((await first.json() as Record<string, unknown>).status).toBe(
      "capabilities_raised",
    );
    expect((await pubkeys(userId)).rn_capabilities).toBe(RN_CAP_WIRE_RN);

    // Any matched UPDATE on this identity now aborts in real D1. This
    // makes "write-free" an observed storage property rather than an
    // inference from the unchanged response and bitmap.
    await env.DB.prepare(
      "CREATE TABLE rn_replay_write_guard (user_id TEXT PRIMARY KEY)",
    ).run();
    await env.DB.prepare(
      "INSERT INTO rn_replay_write_guard (user_id) VALUES (?)",
    ).bind(userId).run();
    await env.DB.prepare(
      `CREATE TRIGGER rn_replay_must_not_update
       BEFORE UPDATE ON users
       WHEN EXISTS (
         SELECT 1 FROM rn_replay_write_guard WHERE user_id = OLD.user_id
       )
       BEGIN
         SELECT RAISE(ABORT, 'RN replay attempted a users write');
       END`,
    ).run();

    let replay: Response;
    try {
      // Replaying the identical body is a no-op, not a second raise.
      replay = await post(raise);
    } finally {
      await env.DB.prepare("DROP TRIGGER rn_replay_must_not_update").run();
      await env.DB.prepare("DROP TABLE rn_replay_write_guard").run();
    }
    expect(replay.status).toBe(200);
    const rj = (await replay.json()) as Record<string, unknown>;
    expect(rj.status).toBe("noop");
    expect(rj.rn_capabilities).toBe(RN_CAP_WIRE_RN);
  });

  it("Case B never lowers, and a legacy re-register cannot strip it", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    await post(await signedRegisterBodyWithCaps(userId, pair, RN_CAP_WIRE_RN | 2));

    // A validly-signed lower bitmap.
    const lower = await post(
      await signedRegisterBodyWithCaps(userId, pair, RN_CAP_WIRE_RN),
    );
    expect(lower.status).toBe(200);
    expect((await lower.json() as Record<string, unknown>).rn_capabilities).toBe(
      RN_CAP_WIRE_RN | 2,
    );

    // A rolled-back build: no bitmap at all. Must not strip.
    const legacy = await post(await signedRegisterBody(userId, pair));
    expect(legacy.status).toBe(200);
    expect((await pubkeys(userId)).rn_capabilities).toBe(RN_CAP_WIRE_RN | 2);
  });

  it("Case B refuses a key change bundled with a capability raise", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    await post(await signedRegisterBody(userId, pair));

    const fields = {
      user_id: userId,
      ik_x25519_pub: STUB_X25519_PUB_B64,
      ik_ed25519_pub: pair.publicKeyB64,
      // Different ML-KEM key alongside the raise.
      ik_mlkem768_pub: btoa(
        String.fromCharCode(...new Uint8Array(1184).fill(0x55)),
      ),
      ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
      rn_capabilities: RN_CAP_WIRE_RN,
    };
    const registration_sig = await signEd25519(
      pair.signingKey,
      buildRegMsg(fields),
    );
    const res = await post({ ...fields, registration_sig });
    expect(res.status).toBe(403);
    // Neither the key nor the bitmap moved.
    const j = await pubkeys(userId);
    expect(j.rn_capabilities).toBe(0);
    expect(j.ik_mlkem768_pub).toBe(STUB_MLKEM_PUB_B64);
  });

  it("a rotation may raise the bitmap and must carry it in ROT_MSG", async () => {
    const oldPair = await generateEd25519Pair();
    const newPair = await generateEd25519Pair();
    const userId = uid();
    await post(await signedRegisterBody(userId, oldPair));

    const fields = {
      user_id: userId,
      ik_x25519_pub: STUB_X25519_PUB_B64,
      ik_ed25519_pub: newPair.publicKeyB64,
      ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
      ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
      rn_capabilities: RN_CAP_WIRE_RN,
    };
    const registration_sig = await signEd25519(
      newPair.signingKey,
      buildRegMsg(fields),
    );
    const rotMsg = buildRotMsg({
      user_id: userId,
      prev_ik_ed25519_pub: oldPair.publicKeyB64,
      new_ik_x25519_pub: fields.ik_x25519_pub,
      new_ik_ed25519_pub: fields.ik_ed25519_pub,
      new_ik_mlkem768_pub: fields.ik_mlkem768_pub,
      new_ik_ratchet_initial_pub: fields.ik_ratchet_initial_pub,
      rn_capabilities: RN_CAP_WIRE_RN,
    });
    const prev_sig = await signEd25519(oldPair.signingKey, rotMsg);
    const res = await post({
      ...fields,
      registration_sig,
      rotation: { prev_ik_ed25519_pub: oldPair.publicKeyB64, prev_sig },
    });
    expect(res.status).toBe(200);
    const j = (await res.json()) as Record<string, unknown>;
    expect(j.status).toBe("rotated");
    expect(j.rn_capabilities).toBe(RN_CAP_WIRE_RN);
    expect((await pubkeys(userId)).rn_capabilities).toBe(RN_CAP_WIRE_RN);
  });

  it("a rotation that would LOWER the bitmap is refused", async () => {
    const oldPair = await generateEd25519Pair();
    const newPair = await generateEd25519Pair();
    const userId = uid();
    await post(await signedRegisterBodyWithCaps(userId, oldPair, RN_CAP_WIRE_RN));

    // A rolled-back build: fully authorised rotation, no bitmap.
    const fields = {
      user_id: userId,
      ik_x25519_pub: STUB_X25519_PUB_B64,
      ik_ed25519_pub: newPair.publicKeyB64,
      ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
      ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
    };
    const registration_sig = await signEd25519(
      newPair.signingKey,
      buildRegMsg(fields),
    );
    const prev_sig = await signEd25519(
      oldPair.signingKey,
      buildRotMsg({
        user_id: userId,
        prev_ik_ed25519_pub: oldPair.publicKeyB64,
        new_ik_x25519_pub: fields.ik_x25519_pub,
        new_ik_ed25519_pub: fields.ik_ed25519_pub,
        new_ik_mlkem768_pub: fields.ik_mlkem768_pub,
        new_ik_ratchet_initial_pub: fields.ik_ratchet_initial_pub,
      }),
    );
    const res = await post({
      ...fields,
      registration_sig,
      rotation: { prev_ik_ed25519_pub: oldPair.publicKeyB64, prev_sig },
    });
    expect(res.status).toBe(409);
    // The identity key did NOT rotate either — the refusal is total.
    const j = await pubkeys(userId);
    expect(j.rn_capabilities).toBe(RN_CAP_WIRE_RN);
    expect(j.ik_ed25519_pub).toBe(oldPair.publicKeyB64);
  });
});
