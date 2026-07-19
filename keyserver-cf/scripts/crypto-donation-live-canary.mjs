#!/usr/bin/env node

/**
 * Local operator client for anonymous Bitcoin and Monero donation canaries.
 *
 * It can request an invoice and observe its node-verified status. It has no
 * wallet RPC, transaction, signing, seed, key, entitlement, or spending path.
 */

import { constants as fsConstants } from "node:fs";
import { lstat, open, realpath, rename, unlink } from "node:fs/promises";
import { createHash, randomUUID } from "node:crypto";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import process from "node:process";

export const API_ORIGIN = "https://keyserver.oslprivacy.com";
const STATE_FORMAT = "osl-crypto-donation-live-canary";
const RECEIPT_FORMAT = "osl-crypto-donation-recorded-receipt";
const FORMAT_VERSION = 1;
const MIN_AMOUNT_CENTS = 100;
const MAX_AMOUNT_CENTS = 1_000_000;
const REQUEST_TIMEOUT_MS = 15_000;
const MAX_RESPONSE_BYTES = 64 * 1024;
const MAX_RATE_LIMIT_RETRIES = 3;
const MAX_RETRY_AFTER_SECONDS = 10 * 60;
const POLL_MS = 8_000;
const POST_EXPIRY_POLL_MS = 30_000;
const CONFIRMATION_GRACE_SECONDS = 24 * 60 * 60;

const INVOICE_ID = /^cdon_[0-9a-f]{32}$/;
const CLAIM_TOKEN = /^[A-Za-z0-9_-]{43}$/;
const ADDRESSES = {
  btc: /^bc1[023456789acdefghjklmnpqrstuvwxyz]{11,87}$/,
  xmr: /^[48][123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz]{94}$/,
};
const DECIMALS = { btc: 8, xmr: 12 };
const CONFIRMATIONS = { btc: 2, xmr: 10 };
const QUOTE_KEYS = [
  "invoice_id",
  "claim_token",
  "payment_method",
  "address",
  "amount_native",
  "amount_atomic",
  "amount_usd_cents",
  "price_locked_at",
  "expires_at",
  "confirmations_required",
];
const STATUS_KEYS = [
  "invoice_id",
  "status",
  "payment_method",
  "amount_usd_cents",
  "expires_at",
];
const STATE_KEYS = ["format", "version", "api_origin", "created_at", "invoice", "receipt"];
const RECEIPT_BINDING_KEYS = ["output_path", "content_sha256"];
const RECEIPT_KEYS = [
  "format",
  "version",
  "api_origin",
  "invoice_id",
  "payment_method",
  "amount_usd_cents",
  "amount_native",
  "amount_atomic",
  "confirmations_required",
  "expires_at",
  "status",
];

function plainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export function hasExactKeys(value, expected) {
  if (!plainObject(value)) return false;
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length
    && actual.every((key, index) => key === wanted[index]);
}

function validAsset(value) {
  return value === "btc" || value === "xmr";
}

function validAmountCents(value) {
  return Number.isSafeInteger(value)
    && value >= MIN_AMOUNT_CENTS
    && value <= MAX_AMOUNT_CENTS;
}

function atomicFromNative(amountNative, decimals) {
  const match = typeof amountNative === "string"
    ? amountNative.match(new RegExp(`^(\\d+)\\.(\\d{${decimals}})$`))
    : null;
  if (!match) return null;
  try {
    return BigInt(`${match[1]}${match[2]}`).toString();
  } catch {
    return null;
  }
}

export function validateQuote(value, asset, amountCents, nowSeconds, allowExpired = false) {
  if (!hasExactKeys(value, QUOTE_KEYS)) throw new Error("donation quote schema is invalid");
  const decimals = DECIMALS[asset];
  const atomic = atomicFromNative(value.amount_native, decimals);
  if (
    !validAsset(asset)
    || !validAmountCents(amountCents)
    || !INVOICE_ID.test(value.invoice_id)
    || !CLAIM_TOKEN.test(value.claim_token)
    || value.payment_method !== asset
    || !ADDRESSES[asset]?.test(value.address)
    || typeof value.amount_atomic !== "string"
    || !/^[1-9]\d*$/.test(value.amount_atomic)
    || atomic !== value.amount_atomic
    || value.amount_usd_cents !== amountCents
    || !Number.isSafeInteger(value.price_locked_at)
    || (!allowExpired && value.price_locked_at < nowSeconds - 15 * 60)
    || (!allowExpired && value.price_locked_at > nowSeconds + 60)
    || !Number.isSafeInteger(value.expires_at)
    || (!allowExpired && value.expires_at <= nowSeconds)
    || (!allowExpired && value.expires_at > nowSeconds + 35 * 60)
    || value.expires_at <= value.price_locked_at
    || value.expires_at - value.price_locked_at > 45 * 60
    || value.confirmations_required !== CONFIRMATIONS[asset]
  ) {
    throw new Error("donation quote values are invalid");
  }
  return Object.fromEntries(QUOTE_KEYS.map((key) => [key, value[key]]));
}

export function validateState(value, nowSeconds) {
  if (!hasExactKeys(value, STATE_KEYS)) throw new Error("state file schema is invalid");
  if (
    value.format !== STATE_FORMAT
    || value.version !== FORMAT_VERSION
    || value.api_origin !== API_ORIGIN
    || !Number.isSafeInteger(value.created_at)
    || value.created_at > nowSeconds + 60
    || !plainObject(value.invoice)
  ) {
    throw new Error("state file values are invalid");
  }
  if (value.receipt !== null && (
    !hasExactKeys(value.receipt, RECEIPT_BINDING_KEYS)
    || typeof value.receipt.output_path !== "string"
    || value.receipt.output_path.length === 0
    || value.receipt.output_path.length > 4096
    || !isAbsolute(value.receipt.output_path)
    || typeof value.receipt.content_sha256 !== "string"
    || !/^[0-9a-f]{64}$/.test(value.receipt.content_sha256)
  )) {
    throw new Error("state receipt binding is invalid");
  }
  const asset = value.invoice.payment_method;
  const amountCents = value.invoice.amount_usd_cents;
  const invoice = validateQuote(value.invoice, asset, amountCents, nowSeconds, true);
  if (
    invoice.price_locked_at < value.created_at - 15 * 60
    || invoice.price_locked_at > value.created_at + 60
  ) {
    throw new Error("state invoice clock is invalid");
  }
  return { ...value, invoice };
}

function validateStatus(value, invoice) {
  if (!hasExactKeys(value, STATUS_KEYS)) throw new Error("donation status schema is invalid");
  if (
    value.invoice_id !== invoice.invoice_id
    || (value.status !== "pending" && value.status !== "recorded")
    || value.payment_method !== invoice.payment_method
    || value.amount_usd_cents !== invoice.amount_usd_cents
    || value.expires_at !== invoice.expires_at
  ) {
    throw new Error("donation status values are invalid");
  }
  return value;
}

function receiptFor(invoice) {
  return {
    format: RECEIPT_FORMAT,
    version: FORMAT_VERSION,
    api_origin: API_ORIGIN,
    invoice_id: invoice.invoice_id,
    payment_method: invoice.payment_method,
    amount_usd_cents: invoice.amount_usd_cents,
    amount_native: invoice.amount_native,
    amount_atomic: invoice.amount_atomic,
    confirmations_required: invoice.confirmations_required,
    expires_at: invoice.expires_at,
    status: "recorded",
  };
}

function receiptContent(invoice) {
  return `${JSON.stringify(receiptFor(invoice), null, 2)}\n`;
}

function sha256Hex(value) {
  return createHash("sha256").update(value, "utf8").digest("hex");
}

function endpoint(pathname) {
  const value = new URL(pathname, API_ORIGIN);
  if (value.origin !== API_ORIGIN || value.pathname !== pathname || value.search || value.hash) {
    throw new Error("fixed API endpoint validation failed");
  }
  return value.href;
}

function defaultTimeoutSignal(milliseconds) {
  return AbortSignal.timeout(milliseconds);
}

async function parseBoundedJson(response) {
  const contentLength = Number.parseInt(response.headers.get("content-length") ?? "", 10);
  if (Number.isFinite(contentLength) && contentLength > MAX_RESPONSE_BYTES) {
    throw new Error("API response is too large");
  }
  const text = await response.text();
  if (Buffer.byteLength(text, "utf8") > MAX_RESPONSE_BYTES) {
    throw new Error("API response is too large");
  }
  try {
    return JSON.parse(text);
  } catch {
    throw new Error("API response is not valid JSON");
  }
}

function retryAfterMilliseconds(response) {
  const parsed = Number.parseInt(response.headers.get("retry-after") ?? "", 10);
  const seconds = Number.isSafeInteger(parsed) ? parsed : 60;
  return Math.min(MAX_RETRY_AFTER_SECONDS, Math.max(1, seconds)) * 1000;
}

async function postJsonResponse(pathname, body, deps) {
  for (let attempt = 0; ; attempt += 1) {
    let response;
    try {
      response = await deps.fetch(endpoint(pathname), {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
        redirect: "error",
        signal: deps.timeoutSignal(REQUEST_TIMEOUT_MS),
      });
    } catch {
      throw new Error("API request failed");
    }
    if (response.status === 429 && attempt < MAX_RATE_LIMIT_RETRIES) {
      await deps.sleep(retryAfterMilliseconds(response));
      continue;
    }
    const parsed = await parseBoundedJson(response);
    if (response.status === 429) throw new Error("API rate limit did not clear");
    return { ok: response.ok, status: response.status, body: parsed };
  }
}

async function postJson(pathname, body, deps) {
  const response = await postJsonResponse(pathname, body, deps);
  if (!response.ok) throw new Error(`API request returned HTTP ${response.status}`);
  return response.body;
}

export function createNativeStorage() {
  function verifyFileStat(stat) {
    if (!stat.isFile() || (stat.mode & 0o077) !== 0 || stat.size > 128 * 1024) {
      throw new Error("private file must be a small regular mode-0600 file");
    }
  }

  async function syncParent(pathname) {
    const parent = dirname(resolve(pathname));
    const handle = await open(
      parent,
      fsConstants.O_RDONLY
        | (fsConstants.O_DIRECTORY ?? 0)
        | (fsConstants.O_NOFOLLOW ?? 0),
    );
    try {
      await handle.sync();
    } finally {
      await handle.close();
    }
  }

  return {
    async createExclusive(pathname, data) {
      const handle = await open(
        pathname,
        fsConstants.O_WRONLY | fsConstants.O_CREAT | fsConstants.O_EXCL | fsConstants.O_NOFOLLOW,
        0o600,
      );
      try {
        await handle.writeFile(data, { encoding: "utf8" });
        await handle.sync();
        await handle.chmod(0o600);
      } finally {
        await handle.close();
      }
      await syncParent(pathname);
    },
    async replace(pathname, data) {
      const before = await lstat(pathname);
      verifyFileStat(before);
      const absolute = resolve(pathname);
      const temporary = join(
        dirname(absolute),
        `.${basename(absolute)}.tmp-${process.pid}-${randomUUID()}`,
      );
      let temporaryCreated = false;
      try {
        const handle = await open(
          temporary,
          fsConstants.O_WRONLY
            | fsConstants.O_CREAT
            | fsConstants.O_EXCL
            | fsConstants.O_NOFOLLOW,
          0o600,
        );
        temporaryCreated = true;
        try {
          await handle.writeFile(data, { encoding: "utf8" });
          await handle.sync();
          await handle.chmod(0o600);
        } finally {
          await handle.close();
        }
        const after = await lstat(pathname);
        verifyFileStat(after);
        if (before.dev !== after.dev || before.ino !== after.ino) {
          throw new Error("private file changed before atomic replacement");
        }
        await rename(temporary, absolute);
        temporaryCreated = false;
        await syncParent(absolute);
      } catch (error) {
        if (temporaryCreated) {
          await unlink(temporary).catch(() => {});
          await syncParent(temporary).catch(() => {});
        }
        throw error;
      }
    },
    async readSecure(pathname) {
      const handle = await open(pathname, fsConstants.O_RDONLY | fsConstants.O_NOFOLLOW);
      try {
        verifyFileStat(await handle.stat());
        return await handle.readFile({ encoding: "utf8" });
      } finally {
        await handle.close();
      }
    },
    async remove(pathname) {
      const before = await lstat(pathname);
      verifyFileStat(before);
      const after = await lstat(pathname);
      if (before.dev !== after.dev || before.ino !== after.ino) {
        throw new Error("private file changed before cleanup");
      }
      await unlink(pathname);
      await syncParent(pathname);
    },
  };
}

async function canonicalPath(pathname, mustExist) {
  const absolute = resolve(pathname);
  if (mustExist) return await realpath(absolute);
  return join(await realpath(dirname(absolute)), basename(absolute));
}

function defaultDependencies(overrides = {}) {
  return {
    fetch: globalThis.fetch,
    now: () => Math.floor(Date.now() / 1000),
    sleep: (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)),
    timeoutSignal: defaultTimeoutSignal,
    storage: createNativeStorage(),
    canonicalPath,
    stdout: (line) => console.log(line),
    ...overrides,
  };
}

export async function createDonationCanary({ asset, amountCents, statePath }, overrides = {}) {
  if (!validAsset(asset)) throw new Error("asset must be btc or xmr");
  if (!validAmountCents(amountCents)) {
    throw new Error("amount-cents must be an integer from 100 through 1000000");
  }
  if (typeof statePath !== "string" || statePath.length === 0) throw new Error("state path is required");
  const deps = defaultDependencies(overrides);
  const createdAt = deps.now();
  const incomplete = `${JSON.stringify({
    format: STATE_FORMAT,
    version: FORMAT_VERSION,
    api_origin: API_ORIGIN,
    created_at: createdAt,
    invoice: null,
    receipt: null,
  })}\n`;
  await deps.storage.createExclusive(statePath, incomplete);
  try {
    const response = await postJson("/v1/donations/crypto/quote", {
      payment_method: asset,
      amount_usd_cents: amountCents,
    }, deps);
    const invoice = validateQuote(response, asset, amountCents, deps.now());
    const state = {
      format: STATE_FORMAT,
      version: FORMAT_VERSION,
      api_origin: API_ORIGIN,
      created_at: createdAt,
      invoice,
      receipt: null,
    };
    await deps.storage.replace(statePath, `${JSON.stringify(state)}\n`);
    deps.stdout(`Invoice: ${invoice.invoice_id}`);
    deps.stdout(`Address: ${invoice.address}`);
    deps.stdout(`Amount: ${invoice.amount_native} ${asset.toUpperCase()}`);
    deps.stdout(`Confirmations: ${invoice.confirmations_required}`);
    deps.stdout(`Expires: ${new Date(invoice.expires_at * 1000).toISOString()}`);
    return invoice;
  } catch (error) {
    await deps.storage.remove(statePath).catch(() => {});
    throw error;
  }
}

async function persistExactReceipt(receiptPath, content, deps) {
  try {
    await deps.storage.createExclusive(receiptPath, content);
    return;
  } catch (error) {
    if (!plainObject(error) || error.code !== "EEXIST") {
      throw new Error("donation receipt could not be written securely");
    }
    let existing;
    try {
      existing = await deps.storage.readSecure(receiptPath);
    } catch {
      throw new Error("donation receipt could not be written securely");
    }
    if (existing !== content) throw new Error("donation receipt already contains different data");
  }
}

function validateExpiredResponse(value) {
  if (!hasExactKeys(value, ["error"]) || value.error !== "invoice expired") {
    throw new Error("expired donation response is invalid");
  }
}

export async function watchDonationCanary({ statePath, receiptOut }, overrides = {}) {
  if (typeof statePath !== "string" || statePath.length === 0) throw new Error("state path is required");
  if (typeof receiptOut !== "string" || receiptOut.length === 0) {
    throw new Error("receipt output path is required");
  }
  if (statePath === receiptOut) throw new Error("receipt output must differ from state path");
  const deps = defaultDependencies(overrides);
  let parsed;
  try {
    parsed = JSON.parse(await deps.storage.readSecure(statePath));
  } catch (error) {
    if (error instanceof SyntaxError) throw new Error("state file is not valid JSON");
    throw error;
  }
  const state = validateState(parsed, deps.now());
  let canonicalStatePath;
  let canonicalReceiptPath;
  try {
    canonicalStatePath = await deps.canonicalPath(statePath, true);
    canonicalReceiptPath = await deps.canonicalPath(receiptOut, false);
  } catch {
    throw new Error("private paths could not be canonicalized");
  }
  if (canonicalStatePath === canonicalReceiptPath) {
    throw new Error("receipt output must differ from state path");
  }
  const expectedReceiptContent = receiptContent(state.invoice);
  const expectedReceiptHash = sha256Hex(expectedReceiptContent);
  if (state.receipt === null) {
    state.receipt = {
      output_path: canonicalReceiptPath,
      content_sha256: expectedReceiptHash,
    };
    await deps.storage.replace(statePath, `${JSON.stringify(state)}\n`);
  } else if (
    state.receipt.output_path !== canonicalReceiptPath
    || state.receipt.content_sha256 !== expectedReceiptHash
  ) {
    throw new Error("receipt output does not match durable state binding");
  }
  const stopAt = state.invoice.expires_at + CONFIRMATION_GRACE_SECONDS;
  let previousStatus = null;

  while (deps.now() < stopAt) {
    const response = await postJsonResponse("/v1/donations/crypto/status", {
      invoice_id: state.invoice.invoice_id,
      claim_token: state.invoice.claim_token,
    }, deps);
    if (response.status === 410) {
      validateExpiredResponse(response.body);
      await deps.storage.remove(statePath);
      deps.stdout("Invoice expired.");
      return { status: "expired" };
    }
    if (!response.ok) throw new Error(`API request returned HTTP ${response.status}`);
    const status = validateStatus(response.body, state.invoice);
    if (status.status !== previousStatus) {
      deps.stdout(`Status: ${status.status}`);
      previousStatus = status.status;
    }
    if (status.status === "recorded") {
      const receipt = receiptFor(state.invoice);
      if (!hasExactKeys(receipt, RECEIPT_KEYS)) throw new Error("internal receipt schema is invalid");
      await persistExactReceipt(canonicalReceiptPath, expectedReceiptContent, deps);
      await deps.storage.remove(statePath);
      deps.stdout("Donation recorded.");
      return { status: "recorded", receipt };
    }
    await deps.sleep(deps.now() < state.invoice.expires_at ? POLL_MS : POST_EXPIRY_POLL_MS);
  }
  throw new Error("donation confirmation window ended");
}

function usage() {
  return [
    "Usage:",
    "  node scripts/crypto-donation-live-canary.mjs create --asset btc|xmr --amount-cents INTEGER --state FILE",
    "  node scripts/crypto-donation-live-canary.mjs watch --state FILE --receipt-out FILE",
  ].join("\n");
}

function parseAmountCents(value) {
  if (typeof value !== "string" || !/^(?:0|[1-9]\d*)$/.test(value)) return null;
  const parsed = Number(value);
  return validAmountCents(parsed) ? parsed : null;
}

function parseArguments(argv) {
  const [command, ...rest] = argv;
  if (command !== "create" && command !== "watch") throw new Error(usage());
  if (rest.length % 2 !== 0) throw new Error(usage());
  const options = {};
  for (let index = 0; index < rest.length; index += 2) {
    const flag = rest[index];
    const value = rest[index + 1];
    if (
      !value
      || !["--asset", "--amount-cents", "--state", "--receipt-out"].includes(flag)
      || options[flag] !== undefined
    ) {
      throw new Error(usage());
    }
    options[flag] = value;
  }
  if (!options["--state"]) throw new Error(usage());
  if (command === "create") {
    const amountCents = parseAmountCents(options["--amount-cents"]);
    if (
      !options["--asset"]
      || amountCents === null
      || options["--receipt-out"] !== undefined
    ) {
      throw new Error(usage());
    }
    return {
      command,
      asset: options["--asset"],
      amountCents,
      statePath: options["--state"],
    };
  }
  if (
    options["--asset"] !== undefined
    || options["--amount-cents"] !== undefined
    || !options["--receipt-out"]
  ) {
    throw new Error(usage());
  }
  return {
    command,
    statePath: options["--state"],
    receiptOut: options["--receipt-out"],
  };
}

export async function runCli(argv = process.argv.slice(2), overrides = {}) {
  const parsed = parseArguments(argv);
  if (parsed.command === "create") {
    return createDonationCanary({
      asset: parsed.asset,
      amountCents: parsed.amountCents,
      statePath: parsed.statePath,
    }, overrides);
  }
  return watchDonationCanary({
    statePath: parsed.statePath,
    receiptOut: parsed.receiptOut,
  }, overrides);
}

const invokedDirectly = process.argv[1]
  && import.meta.url === pathToFileURL(process.argv[1]).href;
if (invokedDirectly) {
  runCli().catch((error) => {
    console.error(`Donation canary stopped safely: ${error instanceof Error ? error.message : "unknown error"}`);
    process.exitCode = 1;
  });
}
