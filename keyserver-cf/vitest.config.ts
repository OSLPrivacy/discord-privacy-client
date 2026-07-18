import path from "node:path";
import {
  cloudflareTest,
  readD1Migrations,
} from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    cloudflareTest(async () => {
      const migrationsPath = path.join(__dirname, "migrations");
      const migrations = await readD1Migrations(migrationsPath);
      return {
        wrangler: { configPath: "./wrangler.toml" },
        miniflare: {
          d1Databases: ["DB"],
          // Rate-limit state is intentionally global to a namespace and
          // is not reset with per-test D1 isolation. Keep integration
          // traffic unthrottled; rate-limit selection/deny/fail-closed
          // semantics have dedicated unit tests with deterministic fakes.
          ratelimits: {
            RATE_LIMIT_5: {
              namespace_id: "1926071401",
              simple: { limit: 100_000, period: 60 },
            },
            RATE_LIMIT_10: {
              namespace_id: "1926071402",
              simple: { limit: 100_000, period: 60 },
            },
            RATE_LIMIT_120: {
              namespace_id: "1926071403",
              simple: { limit: 100_000, period: 60 },
            },
            RATE_LIMIT_1200: {
              namespace_id: "1926071404",
              simple: { limit: 100_000, period: 60 },
            },
            RATE_LIMIT_3600: {
              namespace_id: "1926071405",
              simple: { limit: 100_000, period: 60 },
            },
          },
          bindings: {
              // Distinct operator and client-route deployment gates.
              OSL_KEYSERVER_ADMIN_TOKEN: "test-admin-token-do-not-ship",
              OSL_COMP_ADMIN_TOKEN: "test-comp-admin-token-do-not-ship",
              COMP_AUDIT_HMAC_SECRET: "test-comp-audit-secret-do-not-ship-32-bytes",
              // OSL_KEYSERVER_ALLOWED_USERS retired (open signed
              // register; allowlist removed).
              SELECTOR_MANIFEST_JSON: "",
              // F1.2 — Stripe webhook secret is the only Stripe
              // binding tests need; outbound checkout / portal /
              // Resend calls are exercised by fetchMock or
              // 503-when-unset assertions.
              STRIPE_WEBHOOK_SECRET: "whsec_test_secret",
              LICENSE_HMAC_SECRET: "osl-license-test-secret-v1",
              QA_LICENSE_HMAC_SECRET: "osl-license-qa-test-secret-v1",
              DEPLOYMENT_ENV: "production",
              SUPPORT_EMAIL: "support@oslprivacy.test",
              CRYPTO_WATCHER_URL: "https://watcher.test",
              CRYPTO_WATCHER_REQUEST_SECRET: "test-watcher-request-secret",
              CRYPTO_WATCHER_SETTLEMENT_PUBLIC_KEY: "MCowBQYDK2VwAyEA11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=",
              CRYPTO_BTC_CONFIRMATIONS: "2",
              CRYPTO_XMR_CONFIRMATIONS: "10",
              CRYPTO_PRO_USD_CENTS: "500",
            TEST_MIGRATIONS: migrations,
          },
        },
      };
    }),
  ],
  test: {
    include: ["test/**/*.test.ts"],
    setupFiles: ["./test/apply-migrations.ts"],
  },
});
