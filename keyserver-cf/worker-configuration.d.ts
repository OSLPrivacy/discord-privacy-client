/// <reference types="@cloudflare/workers-types" />
/// <reference types="@cloudflare/vitest-pool-workers" />

import type { Env } from "./src/env";
import type { D1Migration } from "@cloudflare/vitest-pool-workers/config";

declare module "cloudflare:test" {
  // ProvidedEnv is the bindings shape `env` exposes inside tests.
  // Interface merging with the empty interface declared by the
  // pool's types adopts the same fields we declare in src/env.ts,
  // plus the test-only TEST_MIGRATIONS binding read by the
  // setup hook in test/apply-migrations.ts.
  interface ProvidedEnv extends Env {
    TEST_MIGRATIONS: D1Migration[];
  }
}
