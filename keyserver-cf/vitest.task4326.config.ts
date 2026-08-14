import { cloudflareTest } from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./wrangler.task4326.toml" },
    }),
  ],
  test: {
    include: ["test/integration/task_4326_mail_ack.test.ts"],
    testTimeout: 30_000,
  },
});
