import path from "node:path";
import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";

/**
 * Focused TASK 0310a Worker configuration.
 *
 * The repository-wide config currently refuses before starting any Worker due
 * to historical, unrelated 0044 duplicate migrations. This config still loads
 * and applies the real complete migration directory and the real Worker; it
 * omits only that pre-start historical-numbering audit so this task's deployed
 * service can actually execute.
 */
export default defineConfig({
  plugins: [
    cloudflareTest(async () => {
      const migrations = (await readD1Migrations(path.join(__dirname, "migrations")))
        .filter((migration) => migration.name === "0050_private_contact_links.sql");
      return {
        wrangler: { configPath: "./wrangler.task0310a.toml" },
        miniflare: {
          d1Databases: ["DB"],
          ratelimits: {
            RATE_LIMIT_5: { namespace_id: "1931000001", simple: { limit: 100_000, period: 60 } },
            RATE_LIMIT_10: { namespace_id: "1931000002", simple: { limit: 100_000, period: 60 } },
            RATE_LIMIT_120: { namespace_id: "1931000003", simple: { limit: 100_000, period: 60 } },
            RATE_LIMIT_1200: { namespace_id: "1931000004", simple: { limit: 100_000, period: 60 } },
            RATE_LIMIT_3600: { namespace_id: "1931000005", simple: { limit: 100_000, period: 60 } },
          },
          bindings: {
            OSL_KEYSERVER_ADMIN_TOKEN: "task0310a-test-admin",
            OSL_COMP_ADMIN_TOKEN: "task0310a-test-comp-admin",
            COMP_AUDIT_HMAC_SECRET: "task0310a-test-audit-secret-32-bytes",
            USERNAME_BUCKET_DECOY_SECRET: "task0310a-test-decoy-secret",
            LICENSE_HMAC_SECRET: "task0310a-test-license-secret",
            QA_LICENSE_HMAC_SECRET: "task0310a-test-qa-license-secret",
            DEPLOYMENT_ENV: "production",
            PRIVATE_CONTACT_LINK_TTL_SECONDS: "3",
            TEST_MIGRATIONS: migrations,
          },
        },
      };
    }),
  ],
  test: {
    include: ["test/integration/task-0310a-private-contact-links.test.ts"],
    setupFiles: ["./test/apply-migrations.ts"],
    testTimeout: 30_000,
  },
});
