import path from "node:path";
import { fileURLToPath } from "node:url";

const runsKeyserverCfSpec = process.argv.some((arg) =>
  arg.includes("keyserver-cf/") || arg.includes("keyserver-cf\\")
);
const runsKeyserverCfScriptSpec = process.argv.some((arg) =>
  arg.includes("keyserver-cf/scripts/") ||
  arg.includes("keyserver-cf\\scripts\\")
);
const repoRoot = path.dirname(fileURLToPath(import.meta.url));

function runnerVitestRoot(): string {
  const runner = process.argv.find(
    (arg) =>
      arg.endsWith("vitest.mjs") &&
      (arg.includes("/node_modules/vitest/") ||
        arg.includes("\\node_modules\\vitest\\")),
  );
  if (runner) return path.dirname(runner);
  return path.join(repoRoot, "keyserver-cf", "node_modules", "vitest");
}

export default async function config() {
  if (!runsKeyserverCfSpec) return {};
  if (runsKeyserverCfScriptSpec) {
    return {
      root: path.join(repoRoot, "keyserver-cf"),
      test: {
        environment: "node",
        include: ["scripts/**/*.test.ts"],
      },
    };
  }

  const workersPoolUrl = new URL(
    "./keyserver-cf/node_modules/@cloudflare/vitest-pool-workers/dist/pool/index.mjs",
    import.meta.url,
  );
  const { cloudflareTest, readD1Migrations } = await import(
    workersPoolUrl.href
  );
  const migrationsPath = path.join(
    repoRoot,
    "keyserver-cf",
    "migrations",
  );
  const migrations = await readD1Migrations(migrationsPath);

  return {
    root: path.join(repoRoot, "keyserver-cf"),
    resolve: {
      alias: [
        {
          find: /^vitest\/worker$/,
          replacement: path.join(runnerVitestRoot(), "dist", "worker.js"),
        },
        {
          find: /^vitest$/,
          replacement: path.join(runnerVitestRoot(), "dist", "index.js"),
        },
      ],
    },
    plugins: [
      cloudflareTest(async () => ({
        wrangler: { configPath: path.join(repoRoot, "keyserver-cf", "wrangler.toml") },
        miniflare: {
          d1Databases: ["DB"],
          ratelimits: {
            RATE_LIMIT_5: {
              namespace_id: "1926071401",
              simple: { limit: 100_000, period: 60 },
            },
            RATE_LIMIT_10: {
              namespace_id: "1926071402",
              simple: { limit: 100_000, period: 60 },
            },
            RATE_LIMIT_120: {
              namespace_id: "1926071403",
              simple: { limit: 100_000, period: 60 },
            },
            RATE_LIMIT_1200: {
              namespace_id: "1926071404",
              simple: { limit: 100_000, period: 60 },
            },
            RATE_LIMIT_3600: {
              namespace_id: "1926071405",
              simple: { limit: 100_000, period: 60 },
            },
          },
          bindings: {
            OSL_KEYSERVER_ADMIN_TOKEN: "test-admin-token-do-not-ship",
            OSL_COMP_ADMIN_TOKEN: "test-comp-admin-token-do-not-ship",
            COMP_AUDIT_HMAC_SECRET: "test-comp-audit-secret-do-not-ship-32-bytes",
            SELECTOR_MANIFEST_JSON: "",
            STRIPE_WEBHOOK_SECRET: "whsec_test_secret",
            LICENSE_HMAC_SECRET: "osl-license-test-secret-v1",
            QA_LICENSE_HMAC_SECRET: "osl-license-qa-test-secret-v1",
            DEPLOYMENT_ENV: "production",
            SUPPORT_EMAIL: "support@oslprivacy.test",
            CRYPTO_WATCHER_URL: "https://watcher.test",
            CRYPTO_WATCHER_REQUEST_SECRET: "test-watcher-request-secret",
            CRYPTO_WATCHER_SETTLEMENT_PUBLIC_KEY:
              "MCowBQYDK2VwAyEA11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=",
            CRYPTO_BTC_CONFIRMATIONS: "2",
            CRYPTO_BTC_ENABLED: "true",
            CRYPTO_XMR_CONFIRMATIONS: "10",
            CRYPTO_XMR_ENABLED: "true",
            CRYPTO_PRO_USD_CENTS: "500",
            TEST_MIGRATIONS: migrations,
          },
        },
      })),
    ],
    test: {
      include: ["test/**/*.test.ts"],
      setupFiles: ["./test/apply-migrations.ts"],
    },
  };
}
