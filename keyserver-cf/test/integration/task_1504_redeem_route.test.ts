import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { generateLicenseKey } from "../../src/lib/license.js";

const HMAC = "osl-license-test-secret-v1";
const GRANT_SECONDS = 30 * 24 * 60 * 60;

async function seedFixtureCode(): Promise<{ plaintext: string; hash: string }> {
  const { plaintext, hash } = await generateLicenseKey(HMAC);
  const suffix = crypto.randomUUID().slice(0, 8);
  const subscriptionId = `sub_task1504_${suffix}`;
  await env.DB.batch([
    env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_task1504', '', 'ACTIVE', NULL, 0, 1, 1)`,
    ).bind(subscriptionId),
    env.DB.prepare(
      `INSERT INTO licenses (license_hash, subscription_id, issued_at, grant_seconds)
       VALUES (?, ?, 1, ?)`,
    ).bind(hash, subscriptionId, GRANT_SECONDS),
  ]);
  return { plaintext, hash };
}

describe("TASK1504 redeem page route", () => {
  it("returns a success page instead of not found for a valid fixture code", async () => {
    const { plaintext, hash } = await seedFixtureCode();

    const response = await SELF.fetch(
      `http://test/redeem?code=${encodeURIComponent(plaintext)}`,
    );
    const body = await response.text();
    const stored = await env.DB.prepare(
      "SELECT redeemed_at, expires_at FROM licenses WHERE license_hash = ?",
    ).bind(hash).first<{ redeemed_at: number | null; expires_at: number | null }>();

    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toContain("text/html");
    expect(body).toContain("Redemption success");
    expect(body).toContain("OSL Pro redeemed");
    expect(body).not.toContain('"error":"not found"');
    expect(stored?.redeemed_at).toBeTypeOf("number");
    expect(stored?.expires_at).toBe((stored?.redeemed_at as number) + GRANT_SECONDS);

    console.info(
      `TASK1504 direct_redeem_route=/redeem valid_fixture_code=${plaintext} ` +
        `status=${response.status} not_found=${body.includes('"error":"not found"')} ` +
        `page_title=${body.includes("OSL Pro redeemed") ? "OSL Pro redeemed" : "missing"} ` +
        `redeemed_at_type=${typeof stored?.redeemed_at}`,
    );
  });
});
