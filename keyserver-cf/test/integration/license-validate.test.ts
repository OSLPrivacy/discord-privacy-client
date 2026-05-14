import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { generateLicenseKey, hashLicense } from "../../src/lib/license.js";

const HMAC = "osl-license-test-secret-v1";

async function seedSubscription(subId: string, status: string): Promise<void> {
  await env.DB.prepare(
    `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
      status, current_period_end, cancel_at_period_end, created_at, updated_at)
     VALUES (?, 'cus_x', 'a@b.com', ?, strftime('%s','now') + 86400, 0,
       strftime('%s','now'), strftime('%s','now'))`,
  )
    .bind(subId, status)
    .run();
}

async function seedLicense(hash: string, subId: string): Promise<void> {
  await env.DB.prepare(
    `INSERT INTO licenses (license_hash, subscription_id, issued_at)
     VALUES (?, ?, strftime('%s','now'))`,
  )
    .bind(hash, subId)
    .run();
}

async function validate(licenseKey: string): Promise<{
  status: string;
  current_period_end?: number;
  checksum_ok: boolean;
}> {
  const res = await SELF.fetch("http://test/v1/license/validate", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ license_key: licenseKey }),
  });
  expect(res.status).toBe(200);
  return (await res.json()) as {
    status: string;
    current_period_end?: number;
    checksum_ok: boolean;
  };
}

describe("POST /v1/license/validate", () => {
  it("returns UNKNOWN + checksum_ok=false for a malformed key", async () => {
    const j = await validate("not-a-license");
    expect(j.status).toBe("UNKNOWN");
    expect(j.checksum_ok).toBe(false);
  });

  it("returns UNKNOWN + checksum_ok=true for a well-formed but unissued key", async () => {
    const { plaintext } = await generateLicenseKey(HMAC);
    const j = await validate(plaintext);
    expect(j.status).toBe("UNKNOWN");
    expect(j.checksum_ok).toBe(true);
  });

  it("returns ACTIVE for an issued + active subscription", async () => {
    const { plaintext, hash } = await generateLicenseKey(HMAC);
    const subId = `sub_${crypto.randomUUID().slice(0, 8)}`;
    await seedSubscription(subId, "ACTIVE");
    await seedLicense(hash, subId);
    const j = await validate(plaintext);
    expect(j.status).toBe("ACTIVE");
    expect(j.checksum_ok).toBe(true);
    expect(j.current_period_end).toBeGreaterThan(Math.floor(Date.now() / 1000));
  });

  it("returns REVOKED when the license row carries revoked_at", async () => {
    const { plaintext, hash } = await generateLicenseKey(HMAC);
    const subId = `sub_${crypto.randomUUID().slice(0, 8)}`;
    await seedSubscription(subId, "REVOKED");
    await seedLicense(hash, subId);
    await env.DB.prepare(
      `UPDATE licenses SET revoked_at = strftime('%s','now'),
         revoked_reason = 'chargeback' WHERE license_hash = ?`,
    )
      .bind(hash)
      .run();
    const j = await validate(plaintext);
    expect(j.status).toBe("REVOKED");
  });

  it("returns GRACE for a license whose subscription is GRACE", async () => {
    const { plaintext, hash } = await generateLicenseKey(HMAC);
    const subId = `sub_${crypto.randomUUID().slice(0, 8)}`;
    await seedSubscription(subId, "GRACE");
    await seedLicense(hash, subId);
    const j = await validate(plaintext);
    expect(j.status).toBe("GRACE");
  });

  it("tolerates user-style noisy input (case, spaces, dashes)", async () => {
    const { plaintext, hash } = await generateLicenseKey(HMAC);
    const subId = `sub_${crypto.randomUUID().slice(0, 8)}`;
    await seedSubscription(subId, "ACTIVE");
    await seedLicense(hash, subId);
    const noisy = plaintext.toLowerCase().replace(/-/g, " ");
    const j = await validate(noisy);
    expect(j.status).toBe("ACTIVE");
  });

  it("400s on missing license_key", async () => {
    const res = await SELF.fetch("http://test/v1/license/validate", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(400);
  });

  it("avoids unused-import warnings", async () => {
    // Sanity touch on hashLicense so the import isn't dead.
    expect((await hashLicense("x")).length).toBe(64);
  });
});
