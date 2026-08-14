import path from "node:path";
import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    cloudflareTest(async () => {
      const migrations = (await readD1Migrations(path.join(__dirname, "migrations")))
        .filter((migration) => [
          "0004_attachment_capability_digests_and_quota.sql",
          "0006_session_budget_and_atomic_rate_counters.sql",
        ].includes(migration.name));
      return {
        wrangler: { configPath: "./test/task-3680.wrangler.toml" },
        miniflare: {
          d1Databases: ["DB"],
          r2Buckets: ["ATTACHMENTS", "PAYLOADS"],
          kvNamespaces: ["RATE_LIMIT"],
          bindings: {
            RATE_LIMIT_HASH_KEY: "test-rate-limit-hash-key-32-bytes-min",
            TEST_MIGRATIONS: migrations,
          },
        },
      };
    }),
  ],
  test: {
    include: ["test/task-3680-local-cleanup.test.ts"],
    setupFiles: ["./test/apply-migrations.ts"],
    testTimeout: 30_000,
  },
});
