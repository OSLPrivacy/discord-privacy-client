import { defineConfig } from "vitest/config";

// The main config uses @cloudflare/vitest-pool-workers with
// `include: test/**/*.test.ts`, so the 16 specs under scripts/ were collected by
// nothing and had never run. They are plain node tests (node:crypto, node:fs),
// not worker tests, so they need their own node-environment config.
export default defineConfig({
  test: {
    include: ["scripts/**/*.test.ts"],
    environment: "node",
  },
});
