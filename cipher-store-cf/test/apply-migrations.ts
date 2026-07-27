/// Vitest setup hook -- resets per-test storage, then applies all migrations
/// to D1. Without this, D1/R2/KV state leaks between `it(...)` blocks.

import { applyD1Migrations, env, reset } from "cloudflare:test";
import { beforeEach } from "vitest";

beforeEach(async () => {
  await reset();
  await applyD1Migrations(env.DB, env.TEST_MIGRATIONS);
});
