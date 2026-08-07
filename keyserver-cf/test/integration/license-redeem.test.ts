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

async function validate(licenseKey: string): Promise<{
  status: string;
  current_period_end?: number | null;
  checksum_ok: boolean;
}> {
  const response = await SELF.fetch("http://test/v1/license/validate", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ license_key: licenseKey }),
  });
  expect(response.status).toBe(200);
  return await response.json() as {
    status: string;
    current_period_end?: number | null;
    checksum_ok: boolean;
  };
}

describe("POST /v1/license/redeem", () => {
  it("stamps one fixed entitlement period and returns it on retries", async () => {
    const { plaintext, hash } = await seedRedeemableLicense();

    const first = await redeem(plaintext);
    expect(first).toMatchObject({ status: "ACTIVE", checksum_ok: true });
    expect(first.redeemed_at).toBeTypeOf("number");
    expect(first.expires_at).toBe((first.redeemed_at as number) + GRANT_SECONDS);

    const storedFirst = await env.DB.prepare(
      "SELECT redeemed_at, expires_at FROM licenses WHERE license_hash = ?",
    ).bind(hash).first<{ redeemed_at: number; expires_at: number }>();
    expect(storedFirst).toEqual({
      redeemed_at: first.redeemed_at,
      expires_at: first.expires_at,
    });

    // Cross a Unix-second boundary so an unconditional UPDATE cannot hide
    // behind both requests observing the same timestamp.
    await new Promise((resolve) => setTimeout(resolve, 1_100));
    const second = await redeem(plaintext);
    expect(second).toEqual(first);
  });

  it("task_1505_redeeming_fixture_code_records_start_at_entry_time_and_end_date_one_month_later", async () => {
    const { plaintext, hash } = await generateLicenseKey(HMAC);
    const suffix = crypto.randomUUID().slice(0, 8);
    const subscriptionId = `sub_task1505_${suffix}`;
    await env.DB.batch([
      env.DB.prepare(
        `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
          status, current_period_end, cancel_at_period_end, created_at, updated_at)
         VALUES (?, '', '', 'PENDING', NULL, 0, 1, 1)`,
      ).bind(subscriptionId),
      env.DB.prepare(
        `INSERT INTO licenses (license_hash, subscription_id, issued_at, grant_seconds)
         VALUES (?, ?, 1, ?)`,
      ).bind(hash, subscriptionId, GRANT_SECONDS),
    ]);

    const entryStartedAt = Math.floor(Date.now() / 1000);
    const redeemed = await redeem(plaintext);
    const entryFinishedAt = Math.floor(Date.now() / 1000);

    expect(redeemed).toMatchObject({ status: "ACTIVE", checksum_ok: true });
    const redeemedAt = redeemed.redeemed_at;
    expect(redeemedAt).toBeTypeOf("number");
    if (typeof redeemedAt !== "number") throw new Error("redeemed_at was not recorded");
    expect(redeemedAt).toBeGreaterThanOrEqual(entryStartedAt);
    expect(redeemedAt).toBeLessThanOrEqual(entryFinishedAt);
    expect(redeemed.expires_at).toBe(redeemedAt + GRANT_SECONDS);

    const proRecord = await env.DB.prepare(
      `SELECT status, current_period_end, customer_id, customer_email
         FROM subscriptions WHERE subscription_id = ?`,
    ).bind(subscriptionId).first<{
      status: string;
      current_period_end: number | null;
      customer_id: string;
      customer_email: string;
    }>();
    expect(proRecord).toEqual({
      status: "ACTIVE",
      current_period_end: redeemed.expires_at,
      customer_id: "",
      customer_email: "",
    });

    const displayed = await validate(plaintext);
    expect(displayed).toMatchObject({
      status: "ACTIVE",
      current_period_end: redeemed.expires_at,
      checksum_ok: true,
    });

    console.log(`TASK1505 fixture_code_status=${redeemed.status}`);
    console.log(`TASK1505 entry_window_start=${entryStartedAt}`);
    console.log(`TASK1505 recorded_start=${redeemedAt}`);
    console.log(`TASK1505 one_month_seconds=${GRANT_SECONDS}`);
    console.log(`TASK1505 recorded_end=${redeemed.expires_at}`);
    console.log(`TASK1505 pro_record_status=${proRecord?.status}`);
    console.log(`TASK1505 displayed_end=${displayed.current_period_end}`);
    console.log(`TASK1505 stored_card_details=none`);
  });
});
