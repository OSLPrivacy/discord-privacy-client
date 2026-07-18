import { defineConfig } from "vitest/config";

// Operator scripts use the real Node filesystem and must not run inside the
// Cloudflare Workers compatibility pool used by the Worker endpoint tests.
export default defineConfig({
  test: {
    environment: "node",
    include: ["scripts/**/*.test.ts"],
  },
});
