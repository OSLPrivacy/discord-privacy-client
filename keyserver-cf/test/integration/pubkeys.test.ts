import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  generateEd25519Pair,
  signedRegisterBody,
  STUB_MLKEM_PUB_B64,
  STUB_RATCHET_PUB_B64,
  STUB_X25519_PUB_B64,
} from "./helpers.js";

const reservedDerivedId = "osl1_" + "b".repeat(32);

describe("GET /v1/pubkeys/:user_id", () => {
  it("404s for an unknown user_id", async () => {
    const res = await SELF.fetch("http://test/v1/pubkeys/ghost");
    expect(res.status).toBe(404);
  });

  it("404s for a reserved derived-identity id like any unknown id", async () => {
    const pair = await generateEd25519Pair();
    const controlId = "reserved-pubkeys-control";
    const reg = await SELF.fetch("http://test/v1/register", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "cf-connecting-ip": "203.0.113.20",
      },
      body: JSON.stringify(await signedRegisterBody(controlId, pair)),
    });
    expect(reg.status).toBe(201);
    expect((await SELF.fetch(`http://test/v1/pubkeys/${controlId}`)).status).toBe(200);

    const res = await SELF.fetch(`http://test/v1/pubkeys/${reservedDerivedId}`);
    expect(res.status).toBe(404);
    expect(((await res.json()) as { error: string }).error).toBe("unknown user_id");
  });

  it("returns the registered pubkey shape (no admin token required)", async () => {
    // REGISTER-FIX: open + signed registration (no admin header).
    const pair = await generateEd25519Pair();
    const reg = await SELF.fetch("http://test/v1/register", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(await signedRegisterBody("alice-pubkeys", pair)),
    });
    expect(reg.status).toBe(201);
    const res = await SELF.fetch("http://test/v1/pubkeys/alice-pubkeys");
    expect(res.status).toBe(200);
    const j = (await res.json()) as Record<string, unknown>;
    expect(j.user_id).toBe("alice-pubkeys");
    expect(j.ik_x25519_pub).toBe(STUB_X25519_PUB_B64);
    expect(j.ik_ed25519_pub).toBe(pair.publicKeyB64);
    expect(j.ik_mlkem768_pub).toBe(STUB_MLKEM_PUB_B64);
    expect(j.ik_ratchet_initial_pub).toBe(STUB_RATCHET_PUB_B64);
    expect(typeof j.registered_at).toBe("string");
    expect(typeof j.registration_sig).toBe("string");
    // The historical database column name remains private.
    expect(j.ik_x25519_signature).toBeUndefined();
  });

  it("does not publish identity lifecycle timing", async () => {
    // 2026-07-26 audit, "Discord snowflakes expose OSL adoption and a
    // server-visible social graph". Migration 0029 narrowed the namespace so
    // the *set* of identities is no longer enumerable; this closes the other
    // half, where anyone holding an identifier could read that account's
    // lifecycle activity out of a public, unauthenticated route.
    const pair = await generateEd25519Pair();
    const reg = await SELF.fetch("http://test/v1/register", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(await signedRegisterBody("alice-lifecycle", pair)),
    });
    expect(reg.status).toBe(201);

    const res = await SELF.fetch("http://test/v1/pubkeys/alice-lifecycle");
    const j = (await res.json()) as Record<string, unknown>;

    // Rotation timing is the live-activity signal and is gone outright. It is
    // `Option<String>` in every client
    // (crates/keystore/src/client.rs, crates/ipc/src/commands.rs), so its
    // absence deserialises cleanly.
    expect(j).not.toHaveProperty("last_rotated_at");

    // `registered_at` is a required, non-Option field in those same clients, so
    // removing it would 500 every deployed key fetch. It is reduced to UTC date
    // granularity until the client change lands; see
    // docs/reports/server-lane-2026-07-26.md.
    expect(j.registered_at).toMatch(/^\d{4}-\d{2}-\d{2}T00:00:00Z$/);
  });

  it("refuses Discord snowflake lookup even if a legacy row exists", async () => {
    const res = await SELF.fetch(
      "http://test/v1/pubkeys/900000000000000001",
    );
    expect(res.status).toBe(400);
    expect(((await res.json()) as { error: string }).error).toBe(
      "Discord identifiers are not OSL identities",
    );
  });
});
