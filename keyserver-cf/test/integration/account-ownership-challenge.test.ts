import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  ACCOUNT_OWNERSHIP_CHALLENGE_TTL_SECONDS,
  canonicalChallengeBindingBytes,
  sha256Hex,
  type IssuedAccountOwnershipChallenge,
} from "../../src/lib/account-ownership-challenge.js";
import { base64Decode } from "./helpers.js";

const testDb = (env as unknown as { DB: D1Database }).DB;

async function post(body: unknown): Promise<Response> {
  return SELF.fetch("http://test/v1/account-ownership/challenge", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "cf-connecting-ip": "198.51.100.105",
    },
    body: JSON.stringify(body),
  });
}

describe("POST /v1/account-ownership/challenge", () => {
  it("issues a ProofChallenge-shaped Discord challenge and stores only commitments", async () => {
    const serviceAccountId = "900000000000000105";
    const ownerUserId = `osl-owner-${crypto.randomUUID()}`;
    const res = await post({
      service: "discord",
      service_account_id: serviceAccountId,
      owner_user_id: ownerUserId,
      consent: true,
    });

    expect(res.status).toBe(201);
    const body = (await res.json()) as IssuedAccountOwnershipChallenge;
    expect(body).toMatchObject({
      challenge_version: 1,
      service: "discord",
      service_account_id: serviceAccountId,
      owner_user_id: ownerUserId,
      spent: false,
    });
    expect(body.expires_at_unix_seconds - body.issued_at_unix_seconds).toBe(
      ACCOUNT_OWNERSHIP_CHALLENGE_TTL_SECONDS,
    );
    const nonceBytes = base64Decode(body.nonce);
    expect(nonceBytes).toHaveLength(32);

    const row = await testDb
      .prepare(
        `SELECT nonce_sha256, binding_sha256, service, issued_at_unix_seconds,
                expires_at_unix_seconds, spent_at_unix_seconds
           FROM account_ownership_challenges
          WHERE nonce_sha256 = ?`,
      )
      .bind(await sha256Hex(nonceBytes))
      .first<Record<string, unknown>>();
    expect(row).toBeTruthy();
    expect(row).toMatchObject({
      binding_sha256: await sha256Hex(canonicalChallengeBindingBytes(body)),
      service: "discord",
      issued_at_unix_seconds: body.issued_at_unix_seconds,
      expires_at_unix_seconds: body.expires_at_unix_seconds,
      spent_at_unix_seconds: null,
    });
    const stored = JSON.stringify(row);
    expect(stored).not.toContain(serviceAccountId);
    expect(stored).not.toContain(ownerUserId);
    expect(stored).not.toContain(body.nonce);
  });

  it("refuses missing consent instead of treating omission as permission", async () => {
    const res = await post({
      service: "discord",
      service_account_id: "900000000000000106",
      owner_user_id: "owner-without-consent",
    });
    expect(res.status).toBe(403);
    expect(await res.json()).toEqual({
      error: "account ownership challenge requires explicit consent",
    });
  });

  it("refuses absent or malformed binding fields", async () => {
    for (const body of [
      {
        service: "discord",
        service_account_id: "not-a-snowflake",
        owner_user_id: "owner-a",
        consent: true,
      },
      {
        service: "discord",
        service_account_id: "900000000000000107",
        owner_user_id: "900000000000000108",
        consent: true,
      },
      {
        service: "discord",
        service_account_id: "900000000000000109",
        consent: true,
      },
    ]) {
      const res = await post(body);
      expect(res.status).toBe(400);
    }
  });
});
