import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { base64Decode } from "./helpers.js";

function decodeBase64Url(value: string): Uint8Array {
  const padded = value.replaceAll("-", "+").replaceAll("_", "/") + "=";
  return base64Decode(padded);
}

describe("POST /v1/proof-challenge", () => {
  it("Worker endpoint issuing ProofChallenge for a claimed Discord sno", async () => {
    const serviceAccountId = "900000000000000001";
    const ownerUserId = `owner-${crypto.randomUUID()}`;

    const res = await SELF.fetch("http://test/v1/proof-challenge", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        service: "discord",
        service_account_id: serviceAccountId,
        owner_user_id: ownerUserId,
      }),
    });

    expect(res.status).toBe(201);
    const challenge = await res.json() as {
      service: string;
      service_account_id: string;
      owner_user_id: string;
      nonce_b64url: string;
      issued_at_unix_seconds: number;
      expires_at_unix_seconds: number;
      spent: boolean;
    };
    expect(challenge).toMatchObject({
      service: "discord",
      service_account_id: serviceAccountId,
      owner_user_id: ownerUserId,
      spent: false,
    });
    expect(challenge.nonce_b64url).toMatch(/^[A-Za-z0-9_-]{43}$/u);
    expect(decodeBase64Url(challenge.nonce_b64url)).toHaveLength(32);
    expect(challenge.expires_at_unix_seconds - challenge.issued_at_unix_seconds)
      .toBe(300);

    const invalidAccount = await SELF.fetch("http://test/v1/proof-challenge", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        service: "discord",
        service_account_id: "not-a-snowflake",
        owner_user_id: ownerUserId,
      }),
    });
    expect(invalidAccount.status).toBe(400);

    const invalidOwner = await SELF.fetch("http://test/v1/proof-challenge", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        service: "discord",
        service_account_id: serviceAccountId,
        owner_user_id: "900000000000000002",
      }),
    });
    expect(invalidOwner.status).toBe(400);
  });
});
