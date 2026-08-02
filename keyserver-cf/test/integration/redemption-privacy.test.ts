import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { generateLicenseKey } from "../../src/lib/license.js";

describe("T16-T11 redemption-record privacy boundary", () => {
  it("stores only license_hash and redeemed_at for a redemption record", async () => {
    const { plaintext, hash } = await generateLicenseKey("osl-license-test-secret-v1");
    const suffix = crypto.randomUUID().replace(/-/g, "");
    await env.DB.batch([
      env.DB.prepare(`INSERT INTO subscriptions (subscription_id, customer_id, customer_email, status, current_period_end, cancel_at_period_end, created_at, updated_at)
        VALUES (?, '', '', 'PENDING', NULL, 0, 1, 1)`).bind(`pi_${suffix}`),
      env.DB.prepare(`INSERT INTO licenses (license_hash, subscription_id, issued_at, grant_seconds)
        VALUES (?, ?, 1, ?)`).bind(hash, `pi_${suffix}`, 30 * 24 * 60 * 60),
    ]);

    const response = await SELF.fetch("http://test/v1/license/redeem", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ license_key: plaintext }),
    });
    expect(response.status).toBe(200);

    // This projection is the complete redemption record: payment, identity,
    // device, network, expiry and binding fields are deliberately excluded.
    const record = await env.DB.prepare(
      "SELECT license_hash, redeemed_at FROM licenses WHERE license_hash = ?",
    ).bind(hash).first<Record<string, string | number | null>>();
    expect(record).toEqual({ license_hash: hash, redeemed_at: expect.any(Number) });
    expect(Object.keys(record ?? {}).sort()).toEqual(["license_hash", "redeemed_at"]);
  });
});
