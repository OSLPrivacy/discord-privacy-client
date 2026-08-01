import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";

describe("migration 0040 license redemption columns", () => {
  it("keeps legacy license rows nullable while exposing the redemption fields", async () => {
    const columns = await env.DB.prepare("PRAGMA table_info(licenses)")
      .all<{ name: string; type: string; notnull: number }>();
    const byName = new Map(columns.results.map((column) => [column.name, column]));

    expect(byName.get("redeemed_at")).toMatchObject({ type: "INTEGER", notnull: 0 });
    expect(byName.get("grant_seconds")).toMatchObject({ type: "INTEGER", notnull: 0 });
    expect(byName.get("expires_at")).toMatchObject({ type: "INTEGER", notnull: 0 });
    expect(byName.get("redemption_binding")).toMatchObject({ type: "TEXT", notnull: 0 });

    const suffix = crypto.randomUUID().replace(/-/g, "");
    const subscriptionId = `sub_legacy_${suffix}`;
    const licenseHash = `legacy_${suffix}`;
    await env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_legacy', 'legacy@example.test', 'ACTIVE', NULL, 0, 1, 1)`,
    ).bind(subscriptionId).run();
    await env.DB.prepare(
      `INSERT INTO licenses (license_hash, subscription_id, issued_at)
       VALUES (?, ?, 1)`,
    ).bind(licenseHash, subscriptionId).run();

    const legacyRow = await env.DB.prepare(
      `SELECT redeemed_at, grant_seconds, expires_at, redemption_binding
         FROM licenses WHERE license_hash = ?`,
    ).bind(licenseHash).first<Record<string, null>>();
    expect(legacyRow).toEqual({
      redeemed_at: null,
      grant_seconds: null,
      expires_at: null,
      redemption_binding: null,
    });
  });
});
