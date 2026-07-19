#!/usr/bin/env node

/**
 * Private operator client for the two real-payment release canaries.
 *
 * This program can create an invoice and recover its one-time activation
 * delivery. It deliberately has no wallet RPC, seed, signing, transfer, or
 * spending capability. The operator sends the exact invoice amount with an
 * independent wallet.
 */

import { constants as fsConstants } from "node:fs";
import { open, lstat, rename, unlink } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import process from "node:process";
import { randomUUID, webcrypto } from "node:crypto";

export const API_ORIGIN = "https://keyserver.oslprivacy.com";
const STATE_FORMAT = "osl-crypto-live-canary";
const STATE_VERSION = 1;
const REQUEST_TIMEOUT_MS = 15_000;
const MAX_RESPONSE_BYTES = 64 * 1024;
const MAX_RETRY_AFTER_SECONDS = 10 * 60;
const MAX_RATE_LIMIT_RETRIES = 3;
const POLL_MS = 8_000;
const CONFIRMATION_POLL_MS = 30_000;
const CONFIRMATION_GRACE_SECONDS = 24 * 60 * 60;
const PRO_USD_CENTS = 500;

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
const STATUS_KEYS = ["invoice_id", "status", "expires_at", "encrypted_license", "delivery"];
const ACK_KEYS = ["status", "already_acknowledged"];
const VALIDATE_KEYS = ["status", "current_period_end", "checksum_ok"];
const DELIVERY_ENVELOPE_KEYS = [
  "version",
  "invoice_id",
  "payment_method",
  "amount_usd_cents",
  "plan",
  "activation_code",
];
const STATE_KEYS = [
  "format",
  "version",
  "api_origin",
  "created_at",
  "delivery_public_key_spki",
  "invoice",
  "private_jwk",
  "activation",
];
const ACTIVATION_BINDING_KEYS = ["output_path", "content_sha256"];
const INVOICE_ID = /^cpay_[0-9a-f]{32}$/;
const CLAIM_TOKEN = /^[A-Za-z0-9_-]{43}$/;
const LICENSE = /^OSL-[0-9A-HJKMNP-TV-Z]{4}(?:-[0-9A-HJKMNP-TV-Z]{4}){3}$/;
const ADDRESSES = {
  btc: /^bc1[023456789acdefghjklmnpqrstuvwxyz]{11,87}$/,
  xmr: /^[48][123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz]{94}$/,
};
const DECIMALS = { btc: 8, xmr: 12 };
const CONFIRMATIONS = { btc: 2, xmr: 10 };
const TERMINAL_FAILURES = new Set(["expired"]);
const WAITING_STATUSES = new Set(["pending", "paid"]);

function plainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export function hasExactKeys(value, expected) {
  if (!plainObject(value)) return false;
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
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

export function validateQuote(value, asset, nowSeconds, allowExpired = false) {
  if (!hasExactKeys(value, QUOTE_KEYS)) throw new Error("invoice response schema is invalid");
  const decimals = DECIMALS[asset];
  const atomic = atomicFromNative(value.amount_native, decimals);
  if (
    !decimals
    || !INVOICE_ID.test(value.invoice_id)
    || !CLAIM_TOKEN.test(value.claim_token)
    || value.payment_method !== asset
    || !ADDRESSES[asset]?.test(value.address)
    || typeof value.amount_atomic !== "string"
    || !/^[1-9]\d*$/.test(value.amount_atomic)
    || atomic !== value.amount_atomic
    || value.amount_usd_cents !== PRO_USD_CENTS
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
    throw new Error("invoice response values are invalid");
  }
  return Object.fromEntries(QUOTE_KEYS.map((key) => [key, value[key]]));
}

function validatePrivateJwk(value) {
  if (!plainObject(value) || value.kty !== "RSA") throw new Error("state private key is invalid");
  for (const key of ["n", "e", "d", "p", "q", "dp", "dq", "qi"]) {
    if (typeof value[key] !== "string" || !/^[A-Za-z0-9_-]+$/.test(value[key])) {
      throw new Error("state private key is invalid");
    }
  }
  if (value.oth !== undefined) throw new Error("multi-prime RSA keys are not accepted");
  return value;
}

export function validateState(value, nowSeconds) {
  if (!hasExactKeys(value, STATE_KEYS)) throw new Error("state file schema is invalid");
  if (
    value.format !== STATE_FORMAT
    || value.version !== STATE_VERSION
    || value.api_origin !== API_ORIGIN
    || !Number.isSafeInteger(value.created_at)
    || value.created_at > nowSeconds + 60
    || typeof value.delivery_public_key_spki !== "string"
    || !/^[A-Za-z0-9+/]+={0,2}$/.test(value.delivery_public_key_spki)
    || value.delivery_public_key_spki.length > 1024
  ) {
    throw new Error("state file values are invalid");
  }
  const asset = value.invoice?.payment_method;
  if (asset !== "btc" && asset !== "xmr") throw new Error("state invoice asset is invalid");
  const invoice = validateQuote(value.invoice, asset, nowSeconds, true);
  if (
    invoice.price_locked_at < value.created_at - 15 * 60
    || invoice.price_locked_at > value.created_at + 60
  ) {
    throw new Error("state invoice clock is invalid");
  }
  validatePrivateJwk(value.private_jwk);
  if (value.activation !== null) {
    if (
      !hasExactKeys(value.activation, ACTIVATION_BINDING_KEYS)
      || typeof value.activation.output_path !== "string"
      || value.activation.output_path.length === 0
      || typeof value.activation.content_sha256 !== "string"
      || !/^[0-9a-f]{64}$/.test(value.activation.content_sha256)
    ) {
      throw new Error("state activation binding is invalid");
    }
  }
  return { ...value, invoice };
}

function validateStatus(value, invoice) {
  if (!hasExactKeys(value, STATUS_KEYS)) throw new Error("status response schema is invalid");
  if (
    value.invoice_id !== invoice.invoice_id
    || value.expires_at !== invoice.expires_at
    || ![...WAITING_STATUSES, ...TERMINAL_FAILURES, "delivery_ready"].includes(value.status)
  ) {
    throw new Error("status response values are invalid");
  }
  if (value.status === "delivery_ready") {
    if (
      typeof value.encrypted_license !== "string"
      || !/^[A-Za-z0-9+/]+={0,2}$/.test(value.encrypted_license)
      || value.encrypted_license.length < 100
      || value.encrypted_license.length > 8192
      || value.delivery !== "rsa-oaep-sha256"
    ) {
      throw new Error("activation delivery is invalid");
    }
  } else if (value.encrypted_license !== null || value.delivery !== null) {
    throw new Error("unexpected activation delivery");
  }
  return value;
}

function validateActivation(value) {
  if (!hasExactKeys(value, VALIDATE_KEYS)) throw new Error("activation validation schema is invalid");
  if (value.status !== "ACTIVE" || value.checksum_ok !== true || value.current_period_end !== null) {
    throw new Error("activation did not validate as lifetime Pro");
  }
}

function validateAcknowledgement(value) {
  if (!hasExactKeys(value, ACK_KEYS)) throw new Error("acknowledgement schema is invalid");
  if (value.status !== "acknowledged" || typeof value.already_acknowledged !== "boolean") {
    throw new Error("activation delivery was not acknowledged");
  }
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

async function postJsonResponse(pathname, body, deps, rateLimitAttempts = MAX_RATE_LIMIT_RETRIES) {
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
    if (response.status === 429 && attempt < rateLimitAttempts) {
      await deps.sleep(retryAfterMilliseconds(response));
      continue;
    }
    const parsed = await parseBoundedJson(response);
    if (response.status === 429) throw new Error("API rate limit did not clear");
    return { status: response.status, ok: response.ok, body: parsed };
  }
}

async function postJson(pathname, body, deps, rateLimitAttempts = MAX_RATE_LIMIT_RETRIES) {
  const result = await postJsonResponse(pathname, body, deps, rateLimitAttempts);
  if (!result.ok) throw new Error(`API request returned HTTP ${result.status}`);
  return result.body;
}

async function fsyncParentDirectory(pathname) {
  const handle = await open(
    dirname(pathname),
    fsConstants.O_RDONLY | fsConstants.O_DIRECTORY | fsConstants.O_NOFOLLOW,
  );
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

export function createNativeStorage(overrides = {}) {
  const fileOps = {
    open,
    lstat,
    rename,
    unlink,
    syncDirectory: fsyncParentDirectory,
    randomToken: randomUUID,
    ...overrides,
  };

  function verifyFileStat(stat) {
    if (!stat.isFile() || (stat.mode & 0o077) !== 0 || stat.size > 128 * 1024) {
      throw new Error("state file must be a small regular mode-0600 file");
    }
  }

  return {
    async createExclusive(pathname, data) {
      const handle = await fileOps.open(
        pathname,
        fsConstants.O_WRONLY | fsConstants.O_CREAT | fsConstants.O_EXCL | fsConstants.O_NOFOLLOW,
        0o600,
      );
      try {
        await handle.writeFile(data, { encoding: "utf8" });
        await handle.sync();
        await handle.chmod(0o600);
        await handle.close();
      } catch (error) {
        await handle.close().catch(() => {});
        await fileOps.unlink(pathname).catch(() => {});
        await fileOps.syncDirectory(pathname).catch(() => {});
        throw error;
      }
      try {
        await fileOps.syncDirectory(pathname);
      } catch (error) {
        // A file whose directory entry was not durably synced must never be
        // treated as a completed secret output on the next run.
        await fileOps.unlink(pathname).catch(() => {});
        await fileOps.syncDirectory(pathname).catch(() => {});
        throw error;
      }
    },
    async replace(pathname, data) {
      const current = await fileOps.open(pathname, fsConstants.O_RDONLY | fsConstants.O_NOFOLLOW);
      let currentStat;
      try {
        currentStat = await current.stat();
        verifyFileStat(currentStat);
      } finally {
        await current.close();
      }

      const temporary = join(
        dirname(pathname),
        `.${basename(pathname)}.tmp-${process.pid}-${fileOps.randomToken()}`,
      );
      let renamed = false;
      try {
        const replacement = await fileOps.open(
          temporary,
          fsConstants.O_WRONLY | fsConstants.O_CREAT | fsConstants.O_EXCL | fsConstants.O_NOFOLLOW,
          0o600,
        );
        try {
          await replacement.writeFile(data, { encoding: "utf8" });
          await replacement.sync();
          await replacement.chmod(0o600);
        } finally {
          await replacement.close();
        }

        const beforeRename = await fileOps.lstat(pathname);
        verifyFileStat(beforeRename);
        if (beforeRename.dev !== currentStat.dev || beforeRename.ino !== currentStat.ino) {
          throw new Error("state file changed before atomic replacement");
        }
        await fileOps.rename(temporary, pathname);
        renamed = true;
        await fileOps.syncDirectory(pathname);
      } catch (error) {
        if (!renamed) await fileOps.unlink(temporary).catch(() => {});
        throw error;
      }
    },
    async readSecure(pathname) {
      const handle = await fileOps.open(pathname, fsConstants.O_RDONLY | fsConstants.O_NOFOLLOW);
      try {
        verifyFileStat(await handle.stat());
        return await handle.readFile({ encoding: "utf8" });
      } finally {
        await handle.close();
      }
    },
    async remove(pathname) {
      const before = await fileOps.lstat(pathname);
      verifyFileStat(before);
      const after = await fileOps.lstat(pathname);
      if (before.dev !== after.dev || before.ino !== after.ino) {
        throw new Error("state file changed before cleanup");
      }
      await fileOps.unlink(pathname);
      await fileOps.syncDirectory(pathname);
    },
  };
}

function defaultDependencies(overrides = {}) {
  return {
    fetch: globalThis.fetch,
    now: () => Math.floor(Date.now() / 1000),
    sleep: (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)),
    timeoutSignal: defaultTimeoutSignal,
    storage: createNativeStorage(),
    crypto: globalThis.crypto ?? webcrypto,
    stdout: (line) => console.log(line),
    ...overrides,
  };
}

function encodeBase64(bytes) {
  return Buffer.from(bytes).toString("base64");
}

function decodeBase64(value) {
  return new Uint8Array(Buffer.from(value, "base64"));
}

async function generateDeliveryKeys(cryptoApi) {
  const pair = await cryptoApi.subtle.generateKey(
    {
      name: "RSA-OAEP",
      modulusLength: 2048,
      publicExponent: new Uint8Array([1, 0, 1]),
      hash: "SHA-256",
    },
    true,
    ["encrypt", "decrypt"],
  );
  return {
    publicSpki: encodeBase64(await cryptoApi.subtle.exportKey("spki", pair.publicKey)),
    privateJwk: await cryptoApi.subtle.exportKey("jwk", pair.privateKey),
  };
}

export async function createCanary({ asset, statePath }, overrides = {}) {
  if (asset !== "btc" && asset !== "xmr") throw new Error("asset must be btc or xmr");
  if (typeof statePath !== "string" || statePath.length === 0) throw new Error("state path is required");
  const deps = defaultDependencies(overrides);
  const createdAt = deps.now();
  const keys = await generateDeliveryKeys(deps.crypto);
  const incomplete = JSON.stringify({
    format: STATE_FORMAT,
    version: STATE_VERSION,
    api_origin: API_ORIGIN,
    created_at: createdAt,
    delivery_public_key_spki: keys.publicSpki,
    invoice: null,
    private_jwk: keys.privateJwk,
    activation: null,
  });
  await deps.storage.createExclusive(statePath, incomplete);
  try {
    const response = await postJson("/v1/crypto/quote", {
      plan: "pro",
      payment_method: asset,
      delivery_public_key_spki: keys.publicSpki,
    }, deps);
    const invoice = validateQuote(response, asset, deps.now());
    const state = {
      format: STATE_FORMAT,
      version: STATE_VERSION,
      api_origin: API_ORIGIN,
      created_at: createdAt,
      delivery_public_key_spki: keys.publicSpki,
      invoice,
      private_jwk: keys.privateJwk,
      activation: null,
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

async function decryptActivationEnvelope(encryptedLicense, state, deps) {
  let privateKey;
  try {
    privateKey = await deps.crypto.subtle.importKey(
      "jwk",
      validatePrivateJwk(state.private_jwk),
      { name: "RSA-OAEP", hash: "SHA-256" },
      false,
      ["decrypt"],
    );
    const plaintext = await deps.crypto.subtle.decrypt(
      { name: "RSA-OAEP" },
      privateKey,
      decodeBase64(encryptedLicense),
    );
    const serialized = new TextDecoder("utf-8", { fatal: true }).decode(plaintext);
    const envelope = JSON.parse(serialized);
    if (
      !hasExactKeys(envelope, DELIVERY_ENVELOPE_KEYS)
      || envelope.version !== 1
      || envelope.invoice_id !== state.invoice.invoice_id
      || envelope.payment_method !== state.invoice.payment_method
      || envelope.amount_usd_cents !== state.invoice.amount_usd_cents
      || envelope.plan !== "pro"
      || !LICENSE.test(envelope.activation_code)
    ) {
      throw new Error("decrypted activation envelope is invalid");
    }
    return envelope.activation_code;
  } catch {
    throw new Error("activation delivery could not be decrypted or bound to this invoice");
  }
}

async function persistActivation(activationOut, license, deps) {
  const content = `${license}\n`;
  try {
    await deps.storage.createExclusive(activationOut, content);
    return;
  } catch (error) {
    // A network failure after the durable write but before acknowledgement can
    // leave both files behind. A retry may proceed only when the existing
    // private output is secure and byte-for-byte identical to this delivery.
    if (!plainObject(error) || error.code !== "EEXIST") {
      throw new Error("activation output could not be written securely");
    }
    let existing;
    try {
      existing = await deps.storage.readSecure(activationOut);
    } catch {
      throw new Error("activation output could not be written securely");
    }
    if (existing !== content) throw new Error("activation output already contains different data");
  }
}

async function sha256Hex(value, deps) {
  const digest = await deps.crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return Buffer.from(digest).toString("hex");
}

async function readBoundActivation(state, statePath, activationOut, deps) {
  if (state.activation === null) return null;
  if (
    state.activation.output_path !== activationOut
    || activationOut === statePath
  ) {
    throw new Error("activation output does not match durable state binding");
  }
  const content = await deps.storage.readSecure(activationOut);
  const license = content.endsWith("\n") ? content.slice(0, -1) : "";
  if (
    !LICENSE.test(license)
    || content !== `${license}\n`
    || await sha256Hex(content, deps) !== state.activation.content_sha256
  ) {
    throw new Error("activation output does not match durable state binding");
  }
  const activation = await postJson("/v1/license/validate", { license_key: license }, deps);
  validateActivation(activation);
  return license;
}

function validateAlreadyAcknowledged(value) {
  if (!hasExactKeys(value, ["error"]) || value.error !== "crypto delivery already acknowledged") {
    throw new Error("acknowledged delivery response is invalid");
  }
}

async function fetchStatus(state, deps, allowAlreadyAcknowledged) {
  const result = await postJsonResponse("/v1/crypto/status", {
    invoice_id: state.invoice.invoice_id,
    claim_token: state.invoice.claim_token,
  }, deps);
  if (result.ok) return { alreadyAcknowledged: false, status: validateStatus(result.body, state.invoice) };
  if (allowAlreadyAcknowledged && result.status === 410) {
    validateAlreadyAcknowledged(result.body);
    return { alreadyAcknowledged: true, status: null };
  }
  throw new Error(`API request returned HTTP ${result.status}`);
}

export async function watchCanary({ statePath, activationOut }, overrides = {}) {
  if (typeof statePath !== "string" || statePath.length === 0) throw new Error("state path is required");
  if (typeof activationOut !== "string" || activationOut.length === 0) {
    throw new Error("activation output path is required");
  }
  if (activationOut === statePath) throw new Error("activation output must differ from state path");
  const deps = defaultDependencies(overrides);
  let parsed;
  try {
    parsed = JSON.parse(await deps.storage.readSecure(statePath));
  } catch (error) {
    if (error instanceof SyntaxError) throw new Error("state file is not valid JSON");
    throw error;
  }
  const state = validateState(parsed, deps.now());
  const stopAt = state.invoice.expires_at + CONFIRMATION_GRACE_SECONDS;
  let previousStatus = null;

  const boundLicense = await readBoundActivation(state, statePath, activationOut, deps);
  if (boundLicense !== null) {
    const resumed = await fetchStatus(state, deps, true);
    if (resumed.alreadyAcknowledged) {
      await deps.storage.remove(statePath);
      deps.stdout("Activation validated and delivery acknowledged.");
      return { status: "acknowledged" };
    }
    if (resumed.status.status !== "delivery_ready") {
      throw new Error("durable activation exists but delivery is not ready");
    }
    const deliveredLicense = await decryptActivationEnvelope(
      resumed.status.encrypted_license,
      state,
      deps,
    );
    if (deliveredLicense !== boundLicense) {
      throw new Error("activation delivery changed after durable validation");
    }
    const acknowledgement = await postJson("/v1/crypto/status", {
      invoice_id: state.invoice.invoice_id,
      claim_token: state.invoice.claim_token,
      acknowledge_delivery: true,
    }, deps);
    validateAcknowledgement(acknowledgement);
    await deps.storage.remove(statePath);
    deps.stdout("Activation validated and delivery acknowledged.");
    return { status: "acknowledged" };
  }

  // Every explicit operator invocation performs one authoritative status
  // request. The grace deadline stops background polling; it must not prevent
  // recovery when reconciliation made a late delivery available afterward.
  let firstPoll = true;
  while (firstPoll || deps.now() < stopAt) {
    firstPoll = false;
    const fetched = await fetchStatus(state, deps, false);
    const status = fetched.status;
    if (status.status !== previousStatus) {
      deps.stdout(`Status: ${status.status}`);
      previousStatus = status.status;
    }
    if (TERMINAL_FAILURES.has(status.status)) {
      deps.stdout("Invoice expired; reconciliation required. Sensitive state was preserved.");
      return { status: status.status, reconciliation_required: true };
    }
    if (status.status === "delivery_ready") {
      const license = await decryptActivationEnvelope(status.encrypted_license, state, deps);
      const activation = await postJson("/v1/license/validate", { license_key: license }, deps);
      validateActivation(activation);
      await persistActivation(activationOut, license, deps);
      const content = `${license}\n`;
      state.activation = {
        output_path: activationOut,
        content_sha256: await sha256Hex(content, deps),
      };
      await deps.storage.replace(statePath, `${JSON.stringify(state)}\n`);
      const acknowledgement = await postJson("/v1/crypto/status", {
        invoice_id: state.invoice.invoice_id,
        claim_token: state.invoice.claim_token,
        acknowledge_delivery: true,
      }, deps);
      validateAcknowledgement(acknowledgement);
      await deps.storage.remove(statePath);
      deps.stdout("Activation validated and delivery acknowledged.");
      return { status: "acknowledged" };
    }
    if (deps.now() >= stopAt) break;
    const wait = deps.now() < state.invoice.expires_at ? POLL_MS : CONFIRMATION_POLL_MS;
    await deps.sleep(wait);
  }
  throw new Error("invoice confirmation window ended");
}

function usage() {
  return [
    "Usage:",
    "  node scripts/crypto-live-canary.mjs create --asset btc|xmr --state FILE",
    "  node scripts/crypto-live-canary.mjs watch --state FILE --activation-out FILE",
  ].join("\n");
}

function parseArguments(argv) {
  const [command, ...rest] = argv;
  if (command !== "create" && command !== "watch") throw new Error(usage());
  const options = {};
  for (let index = 0; index < rest.length; index += 2) {
    const flag = rest[index];
    const value = rest[index + 1];
    if (!value || !["--asset", "--state", "--activation-out"].includes(flag) ||
        options[flag] !== undefined) {
      throw new Error(usage());
    }
    options[flag] = value;
  }
  if (!options["--state"] ||
      (command === "create" && (!options["--asset"] || options["--activation-out"] !== undefined)) ||
      (command === "watch" && (options["--asset"] !== undefined || !options["--activation-out"]))) {
    throw new Error(usage());
  }
  return {
    command,
    asset: options["--asset"],
    statePath: options["--state"],
    activationOut: options["--activation-out"],
  };
}

export async function runCli(argv = process.argv.slice(2), overrides = {}) {
  const parsed = parseArguments(argv);
  if (parsed.command === "create") {
    return createCanary({ asset: parsed.asset, statePath: parsed.statePath }, overrides);
  }
  return watchCanary({ statePath: parsed.statePath, activationOut: parsed.activationOut }, overrides);
}

const invokedDirectly = process.argv[1]
  && import.meta.url === pathToFileURL(process.argv[1]).href;
if (invokedDirectly) {
  runCli().catch((error) => {
    console.error(`Canary stopped safely: ${error instanceof Error ? error.message : "unknown error"}`);
    process.exitCode = 1;
  });
}
