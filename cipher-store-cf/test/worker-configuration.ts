/// <reference types="@cloudflare/workers-types" />
/// <reference types="@cloudflare/vitest-pool-workers" />

declare namespace Cloudflare {
  interface Env {
    DB: D1Database;
    ATTACHMENTS: R2Bucket;
    RATE_LIMIT: KVNamespace;
    RATE_LIMIT_HASH_KEY: string;
    LINK_GRANT_PUBKEY_B64?: string;
    TEST_MIGRATIONS: import("@cloudflare/vitest-pool-workers").D1Migration[];
  }
}

declare module "cloudflare:test" {
  export const env: Cloudflare.Env;
  /// Drives the deployed Worker through its real `fetch` entry point,
  /// routing and all. The rest of this suite calls handlers directly, which
  /// cannot observe route SHAPE -- and "the capability is never accepted from
  /// the URL" is a claim about routing, not about a handler.
  export const SELF: { fetch: typeof globalThis.fetch };
  export function reset(): Promise<void>;
  export function applyD1Migrations(
    db: D1Database,
    migrations: import("@cloudflare/vitest-pool-workers").D1Migration[],
    migrationsTableName?: string,
  ): Promise<void>;
}

declare module "*?raw" {
  const content: string;
  export default content;
}

declare module "node:url" {
  export function fileURLToPath(path: string | globalThis.URL): string;
}
