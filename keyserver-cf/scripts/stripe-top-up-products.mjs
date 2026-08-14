#!/usr/bin/env node

import process from "node:process";
import { createInterface } from "node:readline/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const STRIPE_API = "https://api.stripe.com/v1";
const LIVE_CREATE_APPROVAL = "--liam-approved-live-create";
const LIVE_CREATE_CONFIRMATION = "CREATE OSL TOP-UP PRODUCTS AND PRICES";

export const TOP_UP_CATALOG = Object.freeze([
  Object.freeze({
    code: "TINY-TEST",
    name: "OSL data top-up TINY-TEST",
    allowanceBytes: 1_000_000_000,
    unitAmountUsdCents: 100,
  }),
  Object.freeze({
    code: "SMALL-TEST",
    name: "OSL data top-up SMALL-TEST",
    allowanceBytes: 10_000_000_000,
    unitAmountUsdCents: 500,
  }),
  Object.freeze({
    code: "LARGE-TEST",
    name: "OSL data top-up LARGE-TEST",
    allowanceBytes: 100_000_000_000,
    unitAmountUsdCents: 2_500,
  }),
]);

export function catalogRows(productId = "not created", priceId = "not created") {
  return TOP_UP_CATALOG.map(
    (pack) => `name=${pack.name} product_id=${productId} price_id=${priceId}`,
  );
}

function writeCatalog(writeOut) {
  writeOut(`${catalogRows().join("\n")}\n`);
}

function formHeaders(secretKey, idempotencyKey) {
  return {
    authorization: `Bearer ${secretKey}`,
    "content-type": "application/x-www-form-urlencoded",
    "idempotency-key": idempotencyKey,
  };
}

async function stripePost(fetcher, secretKey, resource, form, idempotencyKey) {
  if (resource !== "products" && resource !== "prices") {
    throw new Error("top-up provisioning attempted a forbidden Stripe resource");
  }
  const response = await fetcher(`${STRIPE_API}/${resource}`, {
    method: "POST",
    headers: formHeaders(secretKey, idempotencyKey),
    body: form.toString(),
  });
  if (!response.ok) {
    throw new Error(`Stripe ${resource} creation failed (${response.status})`);
  }
  const value = await response.json();
  const prefix = resource === "products" ? "prod_" : "price_";
  if (!value || typeof value.id !== "string" || !value.id.startsWith(prefix)) {
    throw new Error(`Stripe ${resource} response did not contain a valid id`);
  }
  return value.id;
}

async function createTopUpCatalog(secretKey, fetcher) {
  const created = [];
  for (const pack of TOP_UP_CATALOG) {
    const productForm = new URLSearchParams();
    productForm.set("name", pack.name);
    productForm.set("metadata[osl_kind]", "data_top_up");
    productForm.set("metadata[osl_pack]", pack.code);
    productForm.set("metadata[allowance_bytes]", String(pack.allowanceBytes));
    const productId = await stripePost(
      fetcher,
      secretKey,
      "products",
      productForm,
      `osl-data-top-up-product-v1-${pack.code.toLowerCase()}`,
    );

    const priceForm = new URLSearchParams();
    priceForm.set("currency", "usd");
    priceForm.set("unit_amount", String(pack.unitAmountUsdCents));
    priceForm.set("product", productId);
    priceForm.set("metadata[osl_kind]", "data_top_up");
    priceForm.set("metadata[osl_pack]", pack.code);
    priceForm.set("metadata[allowance_bytes]", String(pack.allowanceBytes));
    const priceId = await stripePost(
      fetcher,
      secretKey,
      "prices",
      priceForm,
      `osl-data-top-up-price-v1-${pack.code.toLowerCase()}`,
    );
    created.push({ name: pack.name, productId, priceId });
  }
  return created;
}

function defaultDependencies() {
  return {
    isInputTTY: () => process.stdin.isTTY === true,
    isOutputTTY: () => process.stdout.isTTY === true,
    getSecretKey: () => process.env.STRIPE_SECRET_KEY,
    fetcher: fetch,
    writeOut: (text) => process.stdout.write(text),
    writeError: (text) => process.stderr.write(text),
    async readConfirmation(prompt) {
      const readline = createInterface({ input: process.stdin, output: process.stdout });
      try {
        return await readline.question(prompt);
      } finally {
        readline.close();
      }
    },
  };
}

export async function runTopUpProducts(argv, overrides = {}) {
  const command = argv[0];
  const deps = { ...defaultDependencies(), ...overrides };

  if (command === "charge" || command === "checkout") {
    deps.writeError("live charges are disabled\n");
    return 1;
  }
  if (command === "dry-run" && argv.length === 1) {
    writeCatalog(deps.writeOut);
    return 0;
  }
  if (command !== "live-create") {
    deps.writeError(
      "usage: stripe-top-up-products.mjs <dry-run|live-create|charge|checkout> " +
        "[--liam-approved-live-create]\n",
    );
    return 1;
  }

  // A live key and network client stay unreachable until both independent
  // non-payment gates have passed. Argument order cannot weaken either gate.
  if (!deps.isInputTTY() || !deps.isOutputTTY()) {
    deps.writeError("live create requires a human at an interactive terminal\n");
    return 1;
  }
  if (argv.length !== 2 || argv[1] !== LIVE_CREATE_APPROVAL) {
    deps.writeError("live create requires --liam-approved-live-create\n");
    return 1;
  }

  writeCatalog(deps.writeOut);
  const confirmation = await deps.readConfirmation(
    `Type ${LIVE_CREATE_CONFIRMATION} to create exactly this catalog: `,
  );
  if (confirmation !== LIVE_CREATE_CONFIRMATION) {
    deps.writeError("live create confirmation did not match\n");
    return 1;
  }

  const secretKey = deps.getSecretKey();
  if (typeof secretKey !== "string" || !/^(?:sk|rk)_live_/.test(secretKey)) {
    deps.writeError("a live Stripe secret key is required\n");
    return 1;
  }

  try {
    const created = await createTopUpCatalog(secretKey, deps.fetcher);
    for (const row of created) {
      deps.writeOut(
        `name=${row.name} product_id=${row.productId} price_id=${row.priceId}\n`,
      );
    }
    return 0;
  } catch (error) {
    deps.writeError(`${error instanceof Error ? error.message : String(error)}\n`);
    return 1;
  }
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  process.exitCode = await runTopUpProducts(process.argv.slice(2));
}
