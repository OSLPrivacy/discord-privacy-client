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
    // `noUncheckedIndexedAccess` is on, so `responses[0]` is possibly
    // undefined. Throwing is the honest narrowing: it fails the test loudly if
    // the twenty concurrent redemptions ever produce no first response, where
    // an assertion-free `!` would have let the five checks below run against
    // undefined.
    if (first === undefined) throw new Error("twenty concurrent redemptions returned no response");

    expect(first).toMatchObject({ status: "ACTIVE", checksum_ok: true });
    const redeemedAt = first.redeemed_at;
    expect(redeemedAt).toBeTypeOf("number");
    // Replaces `(first.redeemed_at as number)`. The cast asserted the very
    // thing the line above is testing; this narrows on the real value, so the
    // arithmetic below can never silently become `undefined + GRANT_SECONDS`
    // (NaN), which `toBe` would have reported as an ordinary value mismatch.
    if (typeof redeemedAt !== "number") throw new Error("redeemed_at was not a number");
    expect(first.expires_at).toBe(redeemedAt + GRANT_SECONDS);
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
