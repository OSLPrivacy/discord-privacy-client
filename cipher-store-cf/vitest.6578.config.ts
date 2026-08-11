import path from "node:path";
import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    cloudflareTest(async () => {
      const migrations = await readD1Migrations(path.join(__dirname, "migrations"));
      return {
        wrangler: { configPath: "./wrangler.6578.toml" },
        miniflare: {
        d1Databases: ["DB"],
        r2Buckets: ["ATTACHMENTS", "PAYLOADS"],
        kvNamespaces: ["RATE_LIMIT"],
        bindings: {
          RATE_LIMIT_HASH_KEY: "task-6578-test-rate-limit-key",
          // This focused target does not use blob delete grants. The current
          // lane inherited a separately tracked, duplicate-column 0018 merge
          // conflict; excluding that unrelated migration keeps 6578's real
          // attachment D1/R2 boundary executable without editing user work.
          TEST_MIGRATIONS: migrations.filter((migration) => !migration.name.startsWith("0018_")),
        },
        },
      };
    }),
  ],
  test: {
    include: ["test/task-6578-unattended-retention.test.ts"],
    setupFiles: ["./test/apply-migrations.ts"],
    testTimeout: 30_000,
  },
});
