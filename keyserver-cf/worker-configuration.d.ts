/// <reference types="@cloudflare/workers-types" />
/// <reference types="@cloudflare/vitest-pool-workers" />

import type { Env as WorkerEnv } from "./src/env";
import type { D1Migration } from "@cloudflare/vitest-pool-workers/config";

declare global {
  namespace Cloudflare {
    // `cloudflare:test` exposes `env` as Cloudflare.Env in the current
    // Vitest integration. Extend that generated binding shape with our
    // source Env plus the migration fixture used only by tests.
    interface Env extends WorkerEnv {
      TEST_MIGRATIONS: D1Migration[];
    }
  }
}

export {};
