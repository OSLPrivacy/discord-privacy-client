import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";

const testDb = (env as unknown as { DB: D1Database }).DB;

describe("retired manual crypto storage", () => {
  it("contains no historical PII and cannot accept new rows", async () => {
    const before = await testDb
      .prepare("SELECT COUNT(*) AS count FROM crypto_pending_payments")
      .first<{ count: number }>();
    expect(before?.count).toBe(0);

    await expect(
      testDb
        .prepare(
          `INSERT INTO crypto_pending_payments
             (payment_id, payment_method, plan, amount_usd_cents,
              amount_native, address, customer_email, status, txid,
              created_at, resolved_at)
           VALUES ('retired', 'btc', 'monthly', 500, '0.1',
                   'address', 'person@example.test', 'awaiting',
                   'deadbeef', 1, NULL)`,
        )
        .run(),
    ).rejects.toThrow(/legacy crypto payment storage is retired/i);

    const after = await testDb
      .prepare("SELECT COUNT(*) AS count FROM crypto_pending_payments")
      .first<{ count: number }>();
    expect(after?.count).toBe(0);
  });
});
