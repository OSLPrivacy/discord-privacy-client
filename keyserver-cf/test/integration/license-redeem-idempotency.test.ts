import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { generateLicenseKey } from "../../src/lib/license.js";

const HMAC = "osl-license-test-secret-v1";
const GRANT_SECONDS = 30 * 24 * 60 * 60;

async function seedRedeemableLicense(): Promise<{ plaintext: string; hash: string }> {
  const { plaintext, hash } = await generateLicenseKey(HMAC);
  const suffix = crypto.randomUUID().slice(0, 8);
  const subscriptionId = `sub_redeem_idempotency_${suffix}`;
  await env.DB.batch([
    env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_redeem', '', 'ACTIVE', NULL, 0, 1, 1)`,
    ).bind(subscriptionId),
    env.DB.prepare(
      `INSERT INTO licenses (license_hash, subscription_id, issued_at, grant_seconds)
       VALUES (?, ?, 1, ?)`,
    ).bind(hash, subscriptionId, GRANT_SECONDS),
  ]);
  return { plaintext, hash };
}

async function redeem(licenseKey: string): Promise<{
  status: string;
  redeemed_at?: number;
  expires_at?: number;
  checksum_ok: boolean;
}> {
  const response = await SELF.fetch("http://test/v1/license/redeem", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ license_key: licenseKey }),
  });
  expect(response.status).toBe(200);
  return await response.json() as {
    status: string;
    redeemed_at?: number;
    expires_at?: number;
    checksum_ok: boolean;
  };
}

describe("POST /v1/license/redeem idempotency", () => {
  it("gives twenty concurrent retries one immutable redemption period", async () => {
    const { plaintext, hash } = await seedRedeemableLicense();

    const responses = await Promise.all(
      Array.from({ length: 20 }, () => redeem(plaintext)),
    );
    const first = responses[0];

    expect(first).toMatchObject({ status: "ACTIVE", checksum_ok: true });
    expect(first.redeemed_at).toBeTypeOf("number");
    expect(first.expires_at).toBe((first.redeemed_at as number) + GRANT_SECONDS);
    expect(responses).toEqual(Array.from({ length: 20 }, () => first));

    const stored = await env.DB.prepare(
      "SELECT redeemed_at, expires_at FROM licenses WHERE license_hash = ?",
    ).bind(hash).first<{ redeemed_at: number; expires_at: number }>();
    expect(stored).toEqual({
      redeemed_at: first.redeemed_at,
      expires_at: first.expires_at,
    });
  });
});
