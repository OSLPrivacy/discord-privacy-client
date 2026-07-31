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
    // workerd is roughly 15x slower on the GitHub windows-latest runner than it
    // is locally, and the gap is UNIFORM across this suite rather than isolated
    // to one spec. Against vitest's 5s default that produced a steady drip of
    // one-at-a-time CI failures on specs that were slow, not hung -- measured
    // 5313ms, 5799ms and 30617ms on three different files whose bodies all
    // COMPLETED and were then cut off by the harness. Patching them one by one
    // was chasing the symptom.
    //
    // 30s is the honest knob: it reflects the runner's real speed while staying
    // far below the 6h job limit, so a genuine hang still fails the job in
    // seconds rather than sitting there. Individual bulk-sweep specs that
    // legitimately need longer keep their own explicit override.
    testTimeout: 30_000,
  },
});
