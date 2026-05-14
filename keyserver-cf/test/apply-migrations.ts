/// Vitest setup hook — applies all migrations to the isolated D1
/// before each test file. Without this, tests see an empty database
/// and every query fails with `no such table`.

import { applyD1Migrations, env } from "cloudflare:test";

await applyD1Migrations(env.DB, env.TEST_MIGRATIONS);
