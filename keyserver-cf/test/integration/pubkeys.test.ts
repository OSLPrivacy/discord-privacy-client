import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  ADMIN_HEADERS,
  STUB_MLKEM_PUB_B64,
  STUB_RATCHET_PUB_B64,
  STUB_SIGNATURE_B64,
  STUB_X25519_PUB_B64,
} from "./helpers.js";

describe("GET /v1/pubkeys/:user_id", () => {
  it("404s for an unknown user_id", async () => {
    const res = await SELF.fetch("http://test/v1/pubkeys/ghost");
    expect(res.status).toBe(404);
  });

  it("returns the registered pubkey shape (no admin token required)", async () => {
    await SELF.fetch("http://test/v1/register", {
      method: "POST",
      headers: ADMIN_HEADERS,
      body: JSON.stringify({
        user_id: "alice-pubkeys",
        ik_x25519_pub: STUB_X25519_PUB_B64,
        ik_ed25519_pub: STUB_X25519_PUB_B64,
        ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
        ik_x25519_signature: STUB_SIGNATURE_B64,
        ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
      }),
    });
    const res = await SELF.fetch("http://test/v1/pubkeys/alice-pubkeys");
    expect(res.status).toBe(200);
    const j = (await res.json()) as Record<string, unknown>;
    expect(j.user_id).toBe("alice-pubkeys");
    expect(j.ik_x25519_pub).toBe(STUB_X25519_PUB_B64);
    expect(j.ik_ed25519_pub).toBe(STUB_X25519_PUB_B64);
    expect(j.ik_mlkem768_pub).toBe(STUB_MLKEM_PUB_B64);
    expect(j.ik_ratchet_initial_pub).toBe(STUB_RATCHET_PUB_B64);
    expect(typeof j.registered_at).toBe("string");
    // Public route: must not leak the signature (the original
    // Railway server doesn't include ik_x25519_signature in the
    // SELECT; this test pins that.)
    expect(j.ik_x25519_signature).toBeUndefined();
  });
});
