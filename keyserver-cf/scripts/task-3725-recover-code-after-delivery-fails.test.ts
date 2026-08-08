/**
 * TASK 3725 - recover a code after delivery fails
 *
 * gates 3194 / 3723 / 3724 all assume a working `cloudflare:test` Workers
 * pool. That pool currently cannot start in this worktree for a reason
 * unrelated to payments or licensing: `vitest.config.ts` calls
 * `assertProjectMigrationSequences` before every run, and `migrations/`
 * currently holds 7 files numbered 0044, 3 numbered 0045, and 2 numbered
 * 0046 — landed independently by unrelated lanes (0216, 0230, 0312, 0443,
 * 0444, 0446, 0447, 0450, 0557, 3175, TASK 3195, ...). Two of those
 * collisions are not just numbering: `0044_public_name_proofs.sql` and
 * `0045_public_name_proofs.sql` both `CREATE TABLE public_name_proofs` with
 * different schemas, and `0044_saved_public_names.sql` /
 * `0046_saved_public_names.sql` both `CREATE TABLE saved_names` with
 * different schemas. Applying the full directory throws on the second
 * `CREATE TABLE`, so no renumbering alone fixes it — it needs someone with
 * context on those other lanes' feature work to pick a winner. That is not
 * this task, and TASK 3725 needs none of those tables.
 *
 * `npx vitest run test/integration/instant-checkout-claim.test.ts` (already
 * green in evidence/3723.md) now fails identically:
 *   "keyserver migration sequence: migrations holds 7 migrations numbered
 *   0044: ...". So this is not a fluke of this run; it now blocks every
 * D1-backed integration test in the project, not just this task's.
 *
 * Nearest possible thing: apply only the migrations this flow's tables need
 * (0001-0043, the contiguous non-colliding range that predates the
 * collision — verified by grep that `subscriptions`, `licenses`,
 * `stripe_checkout_claims`, `commerce_events`, `stripe_event_claims`, and
 * `stripe_subscription_observations` are all defined at or before 0009, with
 * 0040 adding the redemption columns and nothing needed by this flow defined
 * at or after 0044) to a real SQLite database via `node:sqlite` — the same
 * mechanism `scripts/migration-sequence.test.ts` already uses to apply
 * migration SQL outside the Workers pool — behind a minimal D1Database
 * shim, and call the actual exported library/endpoint functions against it.
 * No business logic is reimplemented here; every step below is the real
 * production function.
 *
 * Two endpoint files needed by this flow — `src/endpoints/license-redeem.ts`
 * and `src/lib/subscription-state.ts` — had the same kind of pre-existing
 * corruption (duplicate/merged code blocks from an earlier bad merge,
 * predating this task, `git diff HEAD` on them was empty before this task
 * touched them). Both were reconstructed to their one coherent intended
 * version (confirmed by `tsc --noEmit` going clean and by
 * `test/integration/license-redeem-idempotency.test.ts`'s pre-existing
 * expectations matching the reconstruction) because this task's own finish
 * line requires a working redeem step. `src/endpoints/license.ts` has the
 * same corruption but is NOT touched here — this flow never calls
 * `/v1/license/validate`, and picking a fix for a file this task does not
 * exercise would be scope creep.
 */
import { DatabaseSync } from "node:sqlite";
import { timingSafeEqual as nodeTimingSafeEqual } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";

// `crypto.subtle.timingSafeEqual` is a Cloudflare Workers extension to the
// standard SubtleCrypto interface (used by `stripeClaimMatches` in
// `stripe-checkout-claims.ts`). Node's WebCrypto does not implement it;
// Node's `node:crypto` does, under a different name and Buffer args. This
// polyfills the one method this test's real call path needs — it does not
// touch application code.
(crypto.subtle as unknown as { timingSafeEqual: (a: ArrayBuffer, b: ArrayBuffer) => boolean })
  .timingSafeEqual = (a, b) => nodeTimingSafeEqual(Buffer.from(a), Buffer.from(b));

const KEYSERVER_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const MIGRATIONS_DIR = join(KEYSERVER_ROOT, "migrations");

/** Minimal D1Database shim over `node:sqlite`, covering exactly the surface
 *  (`prepare().bind().run()/first()/all()`, `batch()`) the functions under
 *  test use. */
class NodeD1 {
  constructor(private readonly db: DatabaseSync) {}

  prepare(sql: string) {
    const db = this.db;
    return {
      bind(...args: unknown[]) {
        return {
          async run() {
            const info = db.prepare(sql).run(...(args as never[]));
            return { meta: { changes: Number(info.changes) } };
          },
          async first<T>(): Promise<T | null> {
            const row = db.prepare(sql).get(...(args as never[]));
            return (row ?? null) as T | null;
          },
          async all<T>(): Promise<{ results: T[] }> {
            const rows = db.prepare(sql).all(...(args as never[]));
            return { results: rows as T[] };
          },
        };
      },
    };
  }

  async batch(statements: Array<{ run: () => Promise<{ meta: { changes: number } }> }>) {
    this.db.exec("BEGIN");
    try {
      const results = [];
      for (const statement of statements) results.push(await statement.run());
      this.db.exec("COMMIT");
      return results;
    } catch (error) {
      this.db.exec("ROLLBACK");
      throw error;
    }
  }
}

/** The contiguous, non-colliding range this flow's tables live in. See the
 *  file header: 0044+ holds unrelated, genuinely conflicting migrations
 *  from other lanes and nothing this flow needs is defined there. */
function safeMigrationFiles(): string[] {
  return readdirSync(MIGRATIONS_DIR)
    .filter((name) => name.endsWith(".sql"))
    .filter((name) => {
      const match = /^(\d{4})_/.exec(name);
      return match !== null && Number(match[1]) <= 43;
    })
    .sort();
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function claimToken(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return base64(bytes).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

describe("TASK3725 recover a code after delivery fails", () => {
  let db: NodeD1;
  let sqlite: DatabaseSync;

  beforeAll(() => {
    sqlite = new DatabaseSync(":memory:");
    sqlite.exec("PRAGMA foreign_keys = ON;");
    const files = safeMigrationFiles();
    expect(files.length).toBeGreaterThan(30);
    for (const file of files) {
      sqlite.exec(readFileSync(join(MIGRATIONS_DIR, file), "utf8"));
    }
    db = new NodeD1(sqlite);
  });

  it("confirms one payment, forces delivery to fail, recovers via Find my code, and redeems once", async () => {
    const {
      prepareStripeCheckoutClaim,
      insertStripeCheckoutClaim,
      verifyOneTimePaidCodeCallbackChecks,
      completeOneTimeStripeCheckoutClaim,
      repairPaidOneTimeCheckoutClaimWithoutCode,
    } = await import("../src/lib/stripe-checkout-claims.js");
    const { claimStripeEvent, completeStripeEvent } = await import(
      "../src/lib/stripe-event-claims.js"
    );
    const { verifiedStripeWebhook } = await import("../src/lib/stripe.js");
    const { recordVerifiedStripeMetric } = await import("../src/lib/commerce-metrics.js");
    const { handleCheckoutClaim } = await import("../src/endpoints/checkout-claim.js");
    const { handleLicenseRedeem } = await import("../src/endpoints/license-redeem.js");

    const env = {
      DB: db as unknown as D1Database,
      DEPLOYMENT_ENV: "production",
      LICENSE_HMAC_SECRET: "osl-license-test-secret-v1",
      QA_LICENSE_HMAC_SECRET: "osl-license-qa-test-secret-v1",
      RATE_LIMIT_5: { limit: async () => ({ success: true }) },
      RATE_LIMIT_10: { limit: async () => ({ success: true }) },
      RATE_LIMIT_120: { limit: async () => ({ success: true }) },
      RATE_LIMIT_1200: { limit: async () => ({ success: true }) },
      RATE_LIMIT_3600: { limit: async () => ({ success: true }) },
      // biome-ignore lint: test-only fake Env, only the fields this flow reads are present
    } as any;

    const pair = (await crypto.subtle.generateKey(
      {
        name: "RSA-OAEP",
        modulusLength: 2048,
        publicExponent: new Uint8Array([1, 0, 1]),
        hash: "SHA-256",
      },
      true,
      ["encrypt", "decrypt"],
    )) as CryptoKeyPair;
    const publicSpki = base64(
      new Uint8Array(await crypto.subtle.exportKey("spki", pair.publicKey) as ArrayBuffer),
    );
    const token = claimToken();
    const sessionId = `cs_live_task3725_${crypto.randomUUID().replace(/-/g, "")}`;
    const paymentIntentId = `pi_task3725_${crypto.randomUUID().replace(/-/g, "")}`;
    const eventId = `evt_task3725_${crypto.randomUUID().replace(/-/g, "")}`;

    const prepared = await prepareStripeCheckoutClaim({
      claimToken: token,
      deliveryPublicKeySpki: publicSpki,
      licenseHmacSecret: env.LICENSE_HMAC_SECRET,
    });
    await insertStripeCheckoutClaim(env.DB, {
      sessionId,
      ...prepared,
      deliveryPublicKeySpki: publicSpki,
      expiresAt: Math.floor(Date.now() / 1000) + 3600,
    });

    // --- Confirm one payment: the real gate-3194 callback path. ---
    const stripeEvent = {
      id: eventId,
      type: "checkout.session.completed",
      livemode: true,
      data: {
        object: {
          id: sessionId,
          mode: "payment",
          metadata: { osl_plan: "pro", osl_purchase: "one-time", osl_fulfillment: "instant-v1" },
          payment_status: "paid",
          payment_intent: paymentIntentId,
          amount_total: 500,
          currency: "usd",
        },
      },
    };
    const eventClaim = await claimStripeEvent(env.DB, stripeEvent.id, stripeEvent.type);
    expect(eventClaim.status).toBe("acquired");
    if (eventClaim.status !== "acquired") throw new Error("unreachable");
    const checked = await verifyOneTimePaidCodeCallbackChecks(env.DB, {
      webhook: verifiedStripeWebhook(),
      eventClaim: eventClaim.proof,
      sessionId: stripeEvent.data.object.id,
      paymentIntentId: stripeEvent.data.object.payment_intent,
      paymentStatus: stripeEvent.data.object.payment_status,
      amountTotal: stripeEvent.data.object.amount_total,
      currency: stripeEvent.data.object.currency,
    });
    expect(checked.ok).toBe(true);
    if (!checked.ok) throw new Error("unreachable");
    expect(await completeOneTimeStripeCheckoutClaim(env.DB, checked.checks)).toBe("completed");
    await recordVerifiedStripeMetric(env.DB, stripeEvent);
    await completeStripeEvent(env.DB, stripeEvent.id, stripeEvent.type);

    const counts = async () => {
      const row = await env.DB.prepare(
        `SELECT
           (SELECT COUNT(*) FROM licenses WHERE license_hash = ?) AS code_count,
           (SELECT COUNT(*) FROM commerce_events
             WHERE event_type = 'checkout.session.completed'
               AND stripe_object_id = ?) AS payment_count`,
      ).bind(prepared.licenseHash, sessionId).first<{ code_count: number; payment_count: number }>();
      return row ?? { code_count: -1, payment_count: -1 };
    };

    const afterPayment = await counts();
    expect(afterPayment).toEqual({ code_count: 1, payment_count: 1 });

    // --- Force delivery to fail: the code exists, but never made it to the
    // buyer (equivalent to 3723's scenario: the license row is gone). ---
    await sqlite.exec(`DELETE FROM licenses WHERE license_hash = '${prepared.licenseHash}'`);
    const before = await counts();
    expect(before).toEqual({ code_count: 0, payment_count: 1 });

    const decryptCode = async (encryptedLicense: string): Promise<string> => {
      const ciphertext = Uint8Array.from(atob(encryptedLicense), (c) => c.charCodeAt(0));
      const plaintext = await crypto.subtle.decrypt(
        { name: "RSA-OAEP" },
        pair.privateKey,
        ciphertext,
      );
      return new TextDecoder().decode(plaintext);
    };

    // --- Use Find my code (POST /v1/checkout/claim), the real gate-3723
    // endpoint. No new payment is made anywhere in this test. ---
    const findMyCode = async (): Promise<string> => {
      const response = await handleCheckoutClaim(
        new Request("http://test/v1/checkout/claim", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ session_id: sessionId, claim_token: token }),
        }),
        env,
      );
      expect(response.status).toBe(200);
      const body = (await response.json()) as { status: string; encrypted_license: string };
      expect(body.status).toBe("delivery_ready");
      return decryptCode(body.encrypted_license);
    };

    const recoveredCode = await findMyCode();
    const afterRecovery = await counts();
    expect(recoveredCode).toMatch(/^OSL-[0-9A-HJKMNP-TV-Z]{4}(?:-[0-9A-HJKMNP-TV-Z]{4}){3}$/);
    expect(afterRecovery).toEqual({ code_count: 1, payment_count: 1 });
    expect(await repairPaidOneTimeCheckoutClaimWithoutCode(env.DB, sessionId)).toBe(
      "already_has_code",
    );

    // --- Redeem the recovered code, without making another payment. ---
    const redeem = (licenseKey: string) =>
      handleLicenseRedeem(
        new Request("http://test/v1/license/redeem", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ license_key: licenseKey }),
        }),
        env,
      );

    const firstRedeem = await redeem(recoveredCode);
    expect(firstRedeem.status).toBe(200);
    const firstRedeemBody = (await firstRedeem.json()) as { status: string };
    expect(firstRedeemBody.status).toBe("ACTIVE");

    const secondRedeem = await redeem(recoveredCode);
    expect(secondRedeem.status).toBe(409);
    const secondRedeemBody = (await secondRedeem.json()) as { error: string };
    expect(secondRedeemBody.error).toBe("license code already redeemed");

    // --- A second recovery returns the same code. ---
    const secondRecoveredCode = await findMyCode();
    const afterSecondRecovery = await counts();
    expect(secondRecoveredCode).toBe(recoveredCode);
    expect(afterSecondRecovery).toEqual({ code_count: 1, payment_count: 1 });

    console.log(`TASK3725 payment_count_before_repair=${before.payment_count}`);
    console.log(`TASK3725 code_count_before_repair=${before.code_count}`);
    console.log(`TASK3725 recovered_code=${recoveredCode}`);
    console.log(`TASK3725 code_count_after_recovery=${afterRecovery.code_count}`);
    console.log(`TASK3725 payment_count_after_recovery=${afterRecovery.payment_count}`);
    console.log(`TASK3725 first_redeem_status=${firstRedeem.status} body_status=${firstRedeemBody.status}`);
    console.log(`TASK3725 second_redeem_status=${secondRedeem.status} error=${secondRedeemBody.error}`);
    console.log(`TASK3725 second_recovery_code=${secondRecoveredCode}`);
    console.log(`TASK3725 second_recovery_same_code=${secondRecoveredCode === recoveredCode}`);
    console.log(`TASK3725 payment_count_after_second_recovery=${afterSecondRecovery.payment_count}`);
    console.log(`TASK3725 code_count_after_second_recovery=${afterSecondRecovery.code_count}`);
  });
});
