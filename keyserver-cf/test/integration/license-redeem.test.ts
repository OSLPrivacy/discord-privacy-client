import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { generateLicenseKey } from "../../src/lib/license.js";

const HMAC = "osl-license-test-secret-v1";
const GRANT_SECONDS = 30 * 24 * 60 * 60;

async function seedRedeemableLicense(): Promise<{ plaintext: string; hash: string }> {
  const { plaintext, hash } = await generateLicenseKey(HMAC);
  const suffix = crypto.randomUUID().slice(0, 8);
  const subscriptionId = `sub_redeem_${suffix}`;
  await env.DB.batch([
    env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_redeem', '', 'PENDING', NULL, 0, 1, 1)`,
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

describe("POST /v1/license/redeem", () => {
  it("task_1506_refuses_a_repeat_fixture_code_redemption_without_moving_the_end_date", async () => {
    const { plaintext, hash } = await seedRedeemableLicense();

    const firstAttempt = await redeemAttempt(plaintext);
    expect(firstAttempt).toMatchObject({ httpStatus: 200, exitCode: 0 });
    expect(firstAttempt.body).toMatchObject({ status: "ACTIVE", checksum_ok: true });
    expect(firstAttempt.body.redeemed_at).toBeTypeOf("number");
    expect(firstAttempt.body.expires_at).toBe(
      (firstAttempt.body.redeemed_at as number) + GRANT_SECONDS,
    );
    const firstEnd = firstAttempt.body.expires_at as number;

    const storedFirst = await env.DB.prepare(
      `SELECT licenses.redeemed_at, licenses.expires_at,
              subscriptions.status, subscriptions.current_period_end,
              subscriptions.customer_id, subscriptions.customer_email
         FROM licenses
         JOIN subscriptions ON subscriptions.subscription_id = licenses.subscription_id
        WHERE licenses.license_hash = ?`,
    ).bind(hash).first<{
      redeemed_at: number;
      expires_at: number;
      status: string;
      current_period_end: number;
      customer_id: string;
      customer_email: string;
    }>();
    expect(storedFirst).toEqual({
      redeemed_at: firstAttempt.body.redeemed_at,
      expires_at: firstEnd,
      status: "ACTIVE",
      current_period_end: firstEnd,
      customer_id: "cus_redeem",
      customer_email: "",
    });

    // Cross a Unix-second boundary so an unconditional UPDATE cannot hide
    // behind both requests observing the same timestamp.
    await new Promise((resolve) => setTimeout(resolve, 1_100));
    const secondAttempt = await redeemAttempt(plaintext);
    expect(secondAttempt.httpStatus).toBe(409);
    expect(secondAttempt.exitCode).not.toBe(0);
    expect(secondAttempt.body).toEqual({ error: "license code already redeemed" });

    const storedAfterSecond = await env.DB.prepare(
      "SELECT redeemed_at, expires_at FROM licenses WHERE license_hash = ?",
    ).bind(hash).first<{ redeemed_at: number; expires_at: number }>();
    expect(storedAfterSecond).toEqual({
      redeemed_at: firstAttempt.body.redeemed_at,
      expires_at: firstEnd,
    });

    console.log(`TASK1506 first_redemption_status=${firstAttempt.body.status}`);
    console.log(`TASK1506 first_exit_code=${firstAttempt.exitCode}`);
    console.log(`TASK1506 first_end=${firstEnd}`);
    console.log(`TASK1506 second_http_status=${secondAttempt.httpStatus}`);
    console.log(`TASK1506 second_exit_code=${secondAttempt.exitCode}`);
    console.log(`TASK1506 second_error=${secondAttempt.body.error}`);
    console.log(`TASK1506 stored_end_after_second=${storedAfterSecond?.expires_at}`);
    console.log(`TASK1506 end_unchanged=${storedAfterSecond?.expires_at === firstEnd}`);
  });
});
