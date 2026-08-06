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

async function redeemAttempt(licenseKey: string): Promise<{
  httpStatus: number;
  exitCode: number;
  body: Record<string, unknown>;
}> {
  const response = await SELF.fetch("http://test/v1/license/redeem", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ license_key: licenseKey }),
  });
  return {
    httpStatus: response.status,
    exitCode: response.ok ? 0 : 1,
    body: await response.json() as Record<string, unknown>,
  };
}

describe("POST /v1/license/redeem single-use gate", () => {
  it("gives twenty concurrent repeat attempts one successful redemption period", async () => {
    const { plaintext, hash } = await seedRedeemableLicense();

    const attempts = await Promise.all(
      Array.from({ length: 20 }, () => redeemAttempt(plaintext)),
    );
    const winners = attempts.filter((attempt) => attempt.httpStatus === 200);
    const repeats = attempts.filter((attempt) => attempt.httpStatus === 409);
    expect(winners).toHaveLength(1);
    expect(repeats).toHaveLength(19);
    expect(repeats).toEqual(
      Array.from({ length: 19 }, () => ({
        httpStatus: 409,
        exitCode: 1,
        body: { error: "license code already redeemed" },
      })),
    );

    const first = winners[0];
    if (first === undefined) throw new Error("twenty concurrent redemptions produced no winner");
    expect(first).toMatchObject({ httpStatus: 200, exitCode: 0 });
    expect(first.body).toMatchObject({ status: "ACTIVE", checksum_ok: true });
    const redeemedAt = first.body.redeemed_at;
    expect(redeemedAt).toBeTypeOf("number");
    if (typeof redeemedAt !== "number") throw new Error("redeemed_at was not a number");
    expect(first.body.expires_at).toBe(redeemedAt + GRANT_SECONDS);

    const stored = await env.DB.prepare(
      "SELECT redeemed_at, expires_at FROM licenses WHERE license_hash = ?",
    ).bind(hash).first<{ redeemed_at: number; expires_at: number }>();
    expect(stored).toEqual({
      redeemed_at: first.body.redeemed_at,
      expires_at: first.body.expires_at,
    });
  });
});
