import path from "node:path";
import {
  defineWorkersConfig,
  readD1Migrations,
} from "@cloudflare/vitest-pool-workers/config";

export default defineWorkersConfig(async () => {
  const migrationsPath = path.join(__dirname, "migrations");
  const migrations = await readD1Migrations(migrationsPath);
  return {
    test: {
      setupFiles: ["./test/apply-migrations.ts"],
      poolOptions: {
        workers: {
          singleWorker: true,
          wrangler: { configPath: "./wrangler.toml" },
          miniflare: {
            d1Databases: ["DB"],
            kvNamespaces: ["RATE_LIMIT_KV"],
            bindings: {
              OSL_KEYSERVER_ADMIN_TOKEN: "test-admin-token-do-not-ship",
              // Empty allowlist = no allowlist enforcement (matches
              // the Railway dev-mode behaviour). The allowlist code
              // path is exercised by a dedicated unit test in
              // test/unit/auth.test.ts.
              OSL_KEYSERVER_ALLOWED_USERS: "",
              SELECTOR_MANIFEST_JSON: "",
              // F1.2 — Stripe webhook secret is the only Stripe
              // binding tests need; outbound checkout / portal /
              // Resend calls are exercised by fetchMock or
              // 503-when-unset assertions.
              STRIPE_WEBHOOK_SECRET: "whsec_test_secret",
              LICENSE_HMAC_SECRET: "osl-license-test-secret-v1",
              SUPPORT_EMAIL: "support@oslprivacy.test",
              // F1.3 — crypto addresses present so the quote/submit
              // body-validation tests don't short-circuit at the
              // 503 "not configured" gate.
              CRYPTO_BTC_ADDRESS: "bc1qtestaddress0000000000000000000000",
              CRYPTO_XMR_ADDRESS: "47testxmrintegratedaddress00000000000000000",
              CRYPTO_MONTHLY_USD_CENTS: "500",
              CRYPTO_YEARLY_USD_CENTS: "5000",
              TEST_MIGRATIONS: migrations,
            },
          },
        },
      },
    },
  };
});
