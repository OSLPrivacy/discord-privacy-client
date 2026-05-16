import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { buildRegMsg, buildRotMsg } from "../../src/lib/signed-request.js";
import {
  generateEd25519Pair,
  signedRegisterBody,
  signEd25519,
  STUB_MLKEM_PUB_B64,
  STUB_RATCHET_PUB_B64,
  STUB_X25519_PUB_B64,
} from "./helpers.js";

let n = 0;
const uid = () => `u-${Date.now()}-${n++}`;

async function post(body: unknown) {
  return SELF.fetch("http://test/v1/register", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

describe("POST /v1/register — OPEN + Ed25519-signed", () => {
  it("Case A: 201 on first registration with a valid signature (NO admin token)", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    const res = await post(await signedRegisterBody(userId, pair));
    expect(res.status).toBe(201);
    const j = (await res.json()) as { user_id: string; registered_at: string };
    expect(j.user_id).toBe(userId);
    expect(j.registered_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it("rejects a bad signature with 400 (proof-of-key-control)", async () => {
    const pair = await generateEd25519Pair();
    const body = await signedRegisterBody(uid(), pair);
    body.registration_sig = btoa(String.fromCharCode(...new Uint8Array(64)));
    const res = await post(body);
    expect(res.status).toBe(400);
    expect(((await res.json()) as { error: string }).error).toMatch(
      /registration_sig/,
    );
  });

  it("rejects a signature by the WRONG key with 400", async () => {
    const real = await generateEd25519Pair();
    const attacker = await generateEd25519Pair();
    const userId = uid();
    const fields = {
      user_id: userId,
      ik_x25519_pub: STUB_X25519_PUB_B64,
      ik_ed25519_pub: real.publicKeyB64,
      ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
      ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
    };
    const sig = await signEd25519(attacker.signingKey, buildRegMsg(fields));
    expect((await post({ ...fields, registration_sig: sig })).status).toBe(400);
  });

  it("400s missing registration_sig", async () => {
    const pair = await generateEd25519Pair();
    const body = await signedRegisterBody(uid(), pair);
    delete (body as Record<string, unknown>).registration_sig;
    expect((await post(body)).status).toBe(400);
  });

  it('LENGTH VALIDATION: the "AAAA" poison row is now impossible', async () => {
    const pair = await generateEd25519Pair();
    const body = await signedRegisterBody(uid(), pair);
    body.ik_x25519_pub = "AAAA"; // 3 bytes, not 32
    const res = await post(body);
    expect(res.status).toBe(400);
    expect(((await res.json()) as { error: string }).error).toMatch(
      /ik_x25519_pub wrong length/,
    );
  });

  it("400s wrong-length ml-kem / ed25519 / ratchet / sig", async () => {
    const pair = await generateEd25519Pair();
    for (const f of [
      "ik_mlkem768_pub",
      "ik_ed25519_pub",
      "ik_ratchet_initial_pub",
      "registration_sig",
    ]) {
      const body = await signedRegisterBody(uid(), pair);
      body[f] = btoa("short");
      expect((await post(body)).status, `${f} should 400`).toBe(400);
    }
  });

  it("400s invalid base64", async () => {
    const pair = await generateEd25519Pair();
    const body = await signedRegisterBody(uid(), pair);
    body.ik_x25519_pub = "not!base64?";
    expect((await post(body)).status).toBe(400);
  });

  it("Case B: re-register with the SAME key → 200 noop, write-free", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    expect((await post(await signedRegisterBody(userId, pair))).status).toBe(201);
    const res = await post(await signedRegisterBody(userId, pair));
    expect(res.status).toBe(200);
    expect(((await res.json()) as { status: string }).status).toBe("noop");
  });

  it("Case C: REJECTS an unauthenticated overwrite by a different key (403)", async () => {
    const owner = await generateEd25519Pair();
    const attacker = await generateEd25519Pair();
    const userId = uid();
    expect((await post(await signedRegisterBody(userId, owner))).status).toBe(201);
    const res = await post(await signedRegisterBody(userId, attacker));
    expect(res.status).toBe(403);
    expect(((await res.json()) as { error: string }).error).toMatch(
      /different key/,
    );
  });

  it("Case C: REJECTS a rotation signed by the wrong old key (403)", async () => {
    const owner = await generateEd25519Pair();
    const attacker = await generateEd25519Pair();
    const next = await generateEd25519Pair();
    const userId = uid();
    expect((await post(await signedRegisterBody(userId, owner))).status).toBe(201);
    const fields = {
      user_id: userId,
      ik_x25519_pub: STUB_X25519_PUB_B64,
      ik_ed25519_pub: next.publicKeyB64,
      ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
      ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
    };
    const regSig = await signEd25519(next.signingKey, buildRegMsg(fields));
    const rotMsg = buildRotMsg({
      user_id: userId,
      prev_ik_ed25519_pub: owner.publicKeyB64,
      new_ik_x25519_pub: fields.ik_x25519_pub,
      new_ik_ed25519_pub: fields.ik_ed25519_pub,
      new_ik_mlkem768_pub: fields.ik_mlkem768_pub,
      new_ik_ratchet_initial_pub: fields.ik_ratchet_initial_pub,
    });
    const prevSig = await signEd25519(attacker.signingKey, rotMsg);
    const res = await post({
      ...fields,
      registration_sig: regSig,
      rotation: { prev_ik_ed25519_pub: owner.publicKeyB64, prev_sig: prevSig },
    });
    expect(res.status).toBe(403);
  });

  it("Case C: AUTHORIZED rotation by the stored key → 200 rotated, replay-inert", async () => {
    const owner = await generateEd25519Pair();
    const next = await generateEd25519Pair();
    const userId = uid();
    expect((await post(await signedRegisterBody(userId, owner))).status).toBe(201);
    const fields = {
      user_id: userId,
      ik_x25519_pub: STUB_X25519_PUB_B64,
      ik_ed25519_pub: next.publicKeyB64,
      ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
      ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
    };
    const regSig = await signEd25519(next.signingKey, buildRegMsg(fields));
    const rotMsg = buildRotMsg({
      user_id: userId,
      prev_ik_ed25519_pub: owner.publicKeyB64,
      new_ik_x25519_pub: fields.ik_x25519_pub,
      new_ik_ed25519_pub: fields.ik_ed25519_pub,
      new_ik_mlkem768_pub: fields.ik_mlkem768_pub,
      new_ik_ratchet_initial_pub: fields.ik_ratchet_initial_pub,
    });
    const prevSig = await signEd25519(owner.signingKey, rotMsg);
    const rotationBody = {
      ...fields,
      registration_sig: regSig,
      rotation: { prev_ik_ed25519_pub: owner.publicKeyB64, prev_sig: prevSig },
    };
    const res = await post(rotationBody);
    expect(res.status).toBe(200);
    const j = (await res.json()) as { status: string; last_rotated_at: string };
    expect(j.status).toBe("rotated");
    expect(j.last_rotated_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);

    // Replay: stored key is now `next`, so the old rotation's
    // submitted ed25519 == stored ⇒ Case B noop (not a re-rotation).
    const replay = await post(rotationBody);
    expect(replay.status).toBe(200);
    expect(((await replay.json()) as { status: string }).status).toBe("noop");
  });

  it("accepts a missing ik_ratchet_initial_pub", async () => {
    const pair = await generateEd25519Pair();
    const userId = uid();
    const body = await signedRegisterBody(userId, pair);
    delete (body as Record<string, unknown>).ik_ratchet_initial_pub;
    const msg = buildRegMsg({
      user_id: userId,
      ik_x25519_pub: body.ik_x25519_pub as string,
      ik_ed25519_pub: body.ik_ed25519_pub as string,
      ik_mlkem768_pub: body.ik_mlkem768_pub as string,
      ik_ratchet_initial_pub: null,
    });
    body.registration_sig = await signEd25519(pair.signingKey, msg);
    expect((await post(body)).status).toBe(201);
  });
});
