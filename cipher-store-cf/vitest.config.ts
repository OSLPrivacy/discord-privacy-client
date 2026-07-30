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
          r2Buckets: ["ATTACHMENTS"],
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
    include: ["test/**/*.test.ts"],
    setupFiles: ["./test/apply-migrations.ts"],
  },
});
