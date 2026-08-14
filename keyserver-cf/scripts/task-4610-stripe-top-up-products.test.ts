import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { describe, expect, it } from "vitest";
// @ts-expect-error The operator command is deliberately plain JavaScript.
import {
  catalogRows,
  runTopUpProducts,
  TOP_UP_CATALOG,
} from "./stripe-top-up-products.mjs";

const SCRIPT = path.resolve("scripts/stripe-top-up-products.mjs");
const EXPECTED_NAMES = [
  "OSL data top-up TINY-TEST",
  "OSL data top-up SMALL-TEST",
  "OSL data top-up LARGE-TEST",
];

function cli(...args: string[]) {
  return spawnSync(process.execPath, [SCRIPT, ...args], {
    encoding: "utf8",
    stdio: ["pipe", "pipe", "pipe"],
  });
}

function guardedDependencies(options: {
  inputTTY: boolean;
  outputTTY: boolean;
  confirmation?: string;
}) {
  const calls = { environment: 0, fetch: 0 };
  let stdout = "";
  let stderr = "";
  return {
    calls,
    output: () => ({ stdout, stderr }),
    deps: {
      isInputTTY: () => options.inputTTY,
      isOutputTTY: () => options.outputTTY,
      getSecretKey: () => {
        calls.environment += 1;
        return "sk_live_fixture_only";
      },
      fetcher: async () => {
        calls.fetch += 1;
        throw new Error("fetch must remain unreachable");
      },
      writeOut: (text: string) => {
        stdout += text;
      },
      writeError: (text: string) => {
        stderr += text;
      },
      readConfirmation: async () => options.confirmation ?? "",
    },
  };
}

describe("TASK4610 Stripe top-up product guard", () => {
  it("prints exactly three not-created fixture rows in dry-run mode", () => {
    const result = cli("dry-run");
    expect(result.status).toBe(0);
    expect(result.stderr).toBe("");
    expect(result.stdout).toBe(`${catalogRows().join("\n")}\n`);
    const rows = result.stdout.trimEnd().split("\n");
    expect(rows).toHaveLength(3);
    expect(TOP_UP_CATALOG.map((pack: { name: string }) => pack.name)).toEqual(EXPECTED_NAMES);
    for (const [index, row] of rows.entries()) {
      expect(row).toContain(`name=${EXPECTED_NAMES[index]}`);
      expect(row).toContain("product_id=not created");
      expect(row).toContain("price_id=not created");
    }
  });

  it.each(["charge", "checkout"])("refuses the %s command with the exact message", (command) => {
    const result = cli(command);
    expect(result.status).toBe(1);
    expect(result.stdout).toBe("");
    expect(result.stderr).toBe("live charges are disabled\n");
  });

  it("refuses non-interactive live create before environment or Stripe access", async () => {
    for (const tty of [
      { inputTTY: false, outputTTY: true },
      { inputTTY: true, outputTTY: false },
      { inputTTY: false, outputTTY: false },
    ]) {
      const fixture = guardedDependencies(tty);
      const status = await runTopUpProducts(
        ["live-create", "--liam-approved-live-create"],
        fixture.deps,
      );
      expect(status).toBe(1);
      expect(fixture.calls).toEqual({ environment: 0, fetch: 0 });
      expect(fixture.output().stdout).toBe("");
      expect(fixture.output().stderr).toContain("requires a human");
    }
  });

  it("changes zero products and prices without Liam's live-create flag", async () => {
    const fixture = guardedDependencies({ inputTTY: true, outputTTY: true });
    const status = await runTopUpProducts(["live-create"], fixture.deps);
    expect(status).toBe(1);
    expect(fixture.calls).toEqual({ environment: 0, fetch: 0 });
    expect(fixture.output().stdout).toBe("");
    expect(fixture.output().stderr).toContain("--liam-approved-live-create");
    console.log("TASK4610 missing_approval product_count_change=0 price_count_change=0");
  });

  it("shows the exact catalog before confirmation and stops before secrets on a mismatch", async () => {
    const fixture = guardedDependencies({
      inputTTY: true,
      outputTTY: true,
      confirmation: "no",
    });
    const status = await runTopUpProducts(
      ["live-create", "--liam-approved-live-create"],
      fixture.deps,
    );
    expect(status).toBe(1);
    expect(fixture.calls).toEqual({ environment: 0, fetch: 0 });
    expect(fixture.output().stdout).toBe(`${catalogRows().join("\n")}\n`);
  });

  it("uses only product and price POSTs with deterministic idempotency after all gates", async () => {
    const requests: Array<{ url: string; init: RequestInit }> = [];
    let nextProduct = 0;
    let nextPrice = 0;
    const fixture = guardedDependencies({
      inputTTY: true,
      outputTTY: true,
      confirmation: "CREATE OSL TOP-UP PRODUCTS AND PRICES",
    });
    const status = await runTopUpProducts(
      ["live-create", "--liam-approved-live-create"],
      {
        ...fixture.deps,
        fetcher: async (url: string, init: RequestInit) => {
          requests.push({ url, init });
          const isProduct = url.endsWith("/v1/products");
          const id = isProduct ? `prod_fixture_${++nextProduct}` : `price_fixture_${++nextPrice}`;
          return new Response(JSON.stringify({ id }), {
            status: 200,
            headers: { "content-type": "application/json" },
          });
        },
      },
    );
    expect(status).toBe(0);
    expect(fixture.calls.environment).toBe(1);
    expect(requests).toHaveLength(6);
    expect(requests.map(({ url }) => new URL(url).pathname)).toEqual([
      "/v1/products", "/v1/prices",
      "/v1/products", "/v1/prices",
      "/v1/products", "/v1/prices",
    ]);
    expect(requests.every(({ init }) => init.method === "POST")).toBe(true);
    expect(requests.map(({ init }) => (init.headers as Record<string, string>)["idempotency-key"]))
      .toEqual([
        "osl-data-top-up-product-v1-tiny-test",
        "osl-data-top-up-price-v1-tiny-test",
        "osl-data-top-up-product-v1-small-test",
        "osl-data-top-up-price-v1-small-test",
        "osl-data-top-up-product-v1-large-test",
        "osl-data-top-up-price-v1-large-test",
      ]);
  });

  it("has zero source paths from top-up selection to a live charge", () => {
    const source = readFileSync(SCRIPT, "utf8");
    const forbidden = [
      ["checkout", "sessions"].join("/"),
      ["payment", "intents"].join("_"),
      ["v1", "charges"].join("/"),
      ["create", "CheckoutSession"].join(""),
    ];
    const liveChargePaths = forbidden.filter((token) => source.includes(token)).length;
    console.log(`TASK4610 live_charge_paths=${liveChargePaths}`);
    expect(liveChargePaths, "live charge path count").toBe(0);
  });
});
