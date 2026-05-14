import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  ADMIN_HEADERS,
  STUB_MLKEM_PUB_B64,
  STUB_RATCHET_PUB_B64,
  STUB_SIGNATURE_B64,
  STUB_X25519_PUB_B64,
  TEST_ADMIN_TOKEN,
} from "./helpers.js";

const VALID_BODY = {
  user_id: "alice",
  ik_x25519_pub: STUB_X25519_PUB_B64,
  ik_ed25519_pub: STUB_X25519_PUB_B64, // not validated cryptographically here
  ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
  ik_x25519_signature: STUB_SIGNATURE_B64,
  ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
};

async function post(body: unknown, headers: Record<string, string> = ADMIN_HEADERS) {
  return SELF.fetch("http://test/v1/register", {
    method: "POST",
    headers,
    body: JSON.stringify(body),
  });
}

describe("POST /v1/register", () => {
  it("401s without the admin token", async () => {
    const res = await post(VALID_BODY, { "content-type": "application/json" });
    expect(res.status).toBe(401);
  });

  it("401s with a wrong admin token", async () => {
    const res = await post(VALID_BODY, {
      "content-type": "application/json",
      authorization: "Bearer wrong",
    });
    expect(res.status).toBe(401);
  });

  it("201s on first registration with valid body + token", async () => {
    const res = await post(VALID_BODY);
    expect(res.status).toBe(201);
    const j = (await res.json()) as { user_id: string; registered_at?: string };
    expect(j.user_id).toBe("alice");
    expect(j.registered_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it("200s on re-registration with key_rotation_recorded", async () => {
    // First create.
    await post({ ...VALID_BODY, user_id: "alice-reg" });
    // Second call updates.
    const res = await post({ ...VALID_BODY, user_id: "alice-reg" });
    expect(res.status).toBe(200);
    const j = (await res.json()) as {
      user_id: string;
      key_rotation_recorded: boolean;
      last_rotated_at: string;
    };
    expect(j.key_rotation_recorded).toBe(true);
    expect(j.last_rotated_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  // NOTE: the allowlist-403 path is exercised by the pure unit
  // test in test/unit/auth.test.ts. The integration test env runs
  // with an empty allowlist (no enforcement) so every other test
  // can register random per-test user_ids without flake.

  it("400s missing required fields", async () => {
    const { user_id: _omit, ...incomplete } = VALID_BODY;
    void _omit;
    const res = await post(incomplete);
    expect(res.status).toBe(400);
    const j = (await res.json()) as { error: string };
    expect(j.error).toMatch(/user_id/);
  });

  it("400s non-base64 pub fields", async () => {
    const res = await post({ ...VALID_BODY, ik_x25519_pub: "not!base64?" });
    expect(res.status).toBe(400);
  });

  it("accepts a missing ik_ratchet_initial_pub (pre-A2 legacy)", async () => {
    const { ik_ratchet_initial_pub: _omit, ...legacy } = VALID_BODY;
    void _omit;
    const res = await post({ ...legacy, user_id: "alice-legacy" });
    expect(res.status).toBe(201);
  });

  it("ignores adminToken even when present if header is malformed", async () => {
    const res = await SELF.fetch("http://test/v1/register", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        // Missing "Bearer " prefix
        authorization: TEST_ADMIN_TOKEN,
      },
      body: JSON.stringify(VALID_BODY),
    });
    expect(res.status).toBe(401);
  });
});
