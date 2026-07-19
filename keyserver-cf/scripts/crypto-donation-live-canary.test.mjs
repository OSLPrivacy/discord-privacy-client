import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import {
  API_ORIGIN,
  createDonationCanary,
  createNativeStorage,
  runCli,
  validateQuote,
  watchDonationCanary,
} from "./crypto-donation-live-canary.mjs";

const NOW = 1_780_000_000;

function memoryStorage() {
  const files = new Map();
  const modes = new Map();
  return {
    files,
    modes,
    async createExclusive(path, data) {
      if (files.has(path)) {
        const error = new Error("exists");
        error.code = "EEXIST";
        throw error;
      }
      files.set(path, data);
      modes.set(path, 0o600);
    },
    async replace(path, data) {
      if (!files.has(path)) throw new Error("missing");
      files.set(path, data);
    },
    async readSecure(path) {
      if (!files.has(path)) throw new Error("missing");
      return files.get(path);
    },
    async remove(path) {
      if (!files.delete(path)) throw new Error("missing");
      modes.delete(path);
    },
  };
}

function json(body, status = 200, headers = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", ...headers },
  });
}

function validQuote(asset = "btc", amountCents = 500) {
  return {
    invoice_id: `cdon_${asset === "btc" ? "a".repeat(32) : "b".repeat(32)}`,
    claim_token: "C".repeat(43),
    payment_method: asset,
    address: asset === "btc" ? `bc1q${"q".repeat(38)}` : `8${"1".repeat(94)}`,
    amount_native: asset === "btc" ? "0.00008333" : "0.033333333334",
    amount_atomic: asset === "btc" ? "8333" : "33333333334",
    amount_usd_cents: amountCents,
    price_locked_at: NOW,
    expires_at: NOW + 1800,
    confirmations_required: asset === "btc" ? 2 : 10,
  };
}

function validStatus(invoice, status = "pending") {
  return {
    invoice_id: invoice.invoice_id,
    status,
    payment_method: invoice.payment_method,
    amount_usd_cents: invoice.amount_usd_cents,
    expires_at: invoice.expires_at,
  };
}

function dependencies({ fetch, storage = memoryStorage(), logs = [], now = () => NOW, sleeps = [] }) {
  return {
    fetch,
    storage,
    now,
    sleep: async (milliseconds) => { sleeps.push(milliseconds); },
    timeoutSignal: () => undefined,
    canonicalPath: async (pathname) => resolve(pathname),
    stdout: (line) => logs.push(line),
  };
}

async function createState(storage, options = {}) {
  const asset = options.asset ?? "btc";
  const amountCents = options.amountCents ?? 500;
  const statePath = options.statePath ?? "/private/donation.json";
  const logs = options.logs ?? [];
  const calls = [];
  await createDonationCanary({ asset, amountCents, statePath }, dependencies({
    storage,
    logs,
    fetch: async (url, init) => {
      calls.push({ url, body: JSON.parse(init.body) });
      return json(validQuote(asset, amountCents));
    },
  }));
  return { calls, invoice: validQuote(asset, amountCents), statePath };
}

test("create sends only asset and cents, stores claim privately, and logs only public invoice details", async () => {
  const storage = memoryStorage();
  const logs = [];
  const { calls, invoice } = await createState(storage, { asset: "xmr", amountCents: 12_345, logs });

  assert.deepEqual(calls, [{
    url: `${API_ORIGIN}/v1/donations/crypto/quote`,
    body: { payment_method: "xmr", amount_usd_cents: 12_345 },
  }]);
  const state = JSON.parse(storage.files.get("/private/donation.json"));
  assert.equal(state.invoice.claim_token, invoice.claim_token);
  assert.equal(storage.modes.get("/private/donation.json"), 0o600);
  assert.match(logs.join("\n"), /Invoice: cdon_/);
  assert.match(logs.join("\n"), /Address: 8/);
  assert.match(logs.join("\n"), /Amount: 0\.033333333334 XMR/);
  assert.match(logs.join("\n"), /Confirmations: 10/);
  assert.match(logs.join("\n"), /Expires:/);
  assert.doesNotMatch(logs.join("\n"), new RegExp(invoice.claim_token));
});

test("create fails closed on extra quote fields and erases provisional private state", async () => {
  const storage = memoryStorage();
  const logs = [];
  await assert.rejects(
    createDonationCanary({ asset: "btc", amountCents: 500, statePath: "/private/state" },
      dependencies({
        storage,
        logs,
        fetch: async () => json({ ...validQuote(), unexpected: true }),
      })),
    /donation quote schema is invalid/,
  );
  assert.equal(storage.files.size, 0);
  assert.deepEqual(logs, []);
});

test("quote validation binds address, native precision, atomic amount, expiry, and confirmation policy", () => {
  const mutations = [
    { address: "bc1invalid" },
    { amount_native: "0.000083330" },
    { amount_atomic: "8334" },
    { expires_at: NOW },
    { expires_at: NOW + 3600 },
    { price_locked_at: NOW - 901 },
    { confirmations_required: 1 },
    { amount_usd_cents: 501 },
  ];
  for (const mutation of mutations) {
    assert.throws(
      () => validateQuote({ ...validQuote(), ...mutation }, "btc", 500, NOW),
      /donation quote values are invalid/,
    );
  }
});

test("watch writes an exact non-secret recorded receipt before deleting claim state", async () => {
  const storage = memoryStorage();
  const logs = [];
  const { invoice } = await createState(storage);
  logs.length = 0;
  let calls = 0;
  const result = await watchDonationCanary({
    statePath: "/private/donation.json",
    receiptOut: "/private/receipt.json",
  }, dependencies({
    storage,
    logs,
    fetch: async (url, init) => {
      calls += 1;
      assert.equal(url, `${API_ORIGIN}/v1/donations/crypto/status`);
      if (calls === 2) {
        assert.equal(storage.files.has("/private/receipt.json"), false);
        assert.equal(storage.files.has("/private/donation.json"), true);
      }
      assert.deepEqual(JSON.parse(init.body), {
        invoice_id: invoice.invoice_id,
        claim_token: invoice.claim_token,
      });
      return json(validStatus(invoice, calls === 1 ? "pending" : "recorded"));
    },
  }));
  assert.equal(result.status, "recorded");
  assert.equal(storage.files.has("/private/donation.json"), false);
  assert.equal(storage.modes.get("/private/receipt.json"), 0o600);
  const receipt = JSON.parse(storage.files.get("/private/receipt.json"));
  assert.deepEqual(receipt, {
    format: "osl-crypto-donation-recorded-receipt",
    version: 1,
    api_origin: API_ORIGIN,
    invoice_id: invoice.invoice_id,
    payment_method: "btc",
    amount_usd_cents: 500,
    amount_native: invoice.amount_native,
    amount_atomic: invoice.amount_atomic,
    confirmations_required: 2,
    expires_at: invoice.expires_at,
    status: "recorded",
  });
  assert.equal("claim_token" in receipt, false);
  assert.deepEqual(logs, ["Status: pending", "Status: recorded", "Donation recorded."]);
  assert.doesNotMatch(logs.join("\n"), new RegExp(invoice.claim_token));
});

test("watch fails closed on extra or mismatched status fields and writes no receipt", async () => {
  for (const badStatus of [
    (invoice) => ({ ...validStatus(invoice, "recorded"), extra: true }),
    (invoice) => ({ ...validStatus(invoice, "recorded"), amount_usd_cents: 501 }),
    (invoice) => ({ ...validStatus(invoice, "paid") }),
  ]) {
    const storage = memoryStorage();
    const { invoice } = await createState(storage);
    await assert.rejects(
      watchDonationCanary({
        statePath: "/private/donation.json",
        receiptOut: "/private/receipt.json",
      }, dependencies({ storage, fetch: async () => json(badStatus(invoice)) })),
      /donation status (?:schema is|values are) invalid/,
    );
    assert.equal(storage.files.has("/private/donation.json"), true);
    assert.equal(storage.files.has("/private/receipt.json"), false);
  }
});

test("receipt output failure preserves claim state and never reports success", async () => {
  const storage = memoryStorage();
  const logs = [];
  const { invoice } = await createState(storage);
  const originalCreate = storage.createExclusive;
  storage.createExclusive = async (path, data) => {
    if (path === "/private/receipt.json") throw new Error("disk full");
    return originalCreate.call(storage, path, data);
  };
  await assert.rejects(
    watchDonationCanary({
      statePath: "/private/donation.json",
      receiptOut: "/private/receipt.json",
    }, dependencies({ storage, logs, fetch: async () => json(validStatus(invoice, "recorded")) })),
    /donation receipt could not be written securely/,
  );
  assert.equal(storage.files.has("/private/donation.json"), true);
  assert.equal(storage.files.has("/private/receipt.json"), false);
  assert.equal(logs.includes("Donation recorded."), false);
});

test("lost response and interrupted cleanup resume without replacing or duplicating a receipt", async () => {
  const storage = memoryStorage();
  const { invoice } = await createState(storage);
  let fetchCalls = 0;
  await assert.rejects(
    watchDonationCanary({
      statePath: "/private/donation.json",
      receiptOut: "/private/receipt.json",
    }, dependencies({
      storage,
      fetch: async () => {
        fetchCalls += 1;
        throw new Error("response lost");
      },
    })),
    /API request failed/,
  );
  assert.equal(fetchCalls, 1);
  assert.equal(storage.files.has("/private/donation.json"), true);
  assert.equal(storage.files.has("/private/receipt.json"), false);

  const originalRemove = storage.remove;
  let failCleanup = true;
  storage.remove = async (path) => {
    if (path === "/private/donation.json" && failCleanup) {
      failCleanup = false;
      throw new Error("simulated cleanup interruption");
    }
    return originalRemove.call(storage, path);
  };
  await assert.rejects(
    watchDonationCanary({
      statePath: "/private/donation.json",
      receiptOut: "/private/receipt.json",
    }, dependencies({ storage, fetch: async () => json(validStatus(invoice, "recorded")) })),
    /simulated cleanup interruption/,
  );
  const firstReceipt = storage.files.get("/private/receipt.json");
  assert.equal(storage.files.has("/private/donation.json"), true);

  let redirectedFetches = 0;
  await assert.rejects(
    watchDonationCanary({
      statePath: "/private/donation.json",
      receiptOut: "/private/redirected-receipt.json",
    }, dependencies({
      storage,
      fetch: async () => {
        redirectedFetches += 1;
        return json(validStatus(invoice, "recorded"));
      },
    })),
    /receipt output does not match durable state binding/,
  );
  assert.equal(redirectedFetches, 0);
  assert.equal(storage.files.has("/private/donation.json"), true);
  assert.equal(storage.files.has("/private/redirected-receipt.json"), false);

  const resumed = await watchDonationCanary({
    statePath: "/private/donation.json",
    receiptOut: "/private/receipt.json",
  }, dependencies({ storage, fetch: async () => json(validStatus(invoice, "recorded")) }));
  assert.equal(resumed.status, "recorded");
  assert.equal(storage.files.get("/private/receipt.json"), firstReceipt);
  assert.equal(storage.files.has("/private/donation.json"), false);
});

test("receipt binding is durably written before the first status poll", async () => {
  const storage = memoryStorage();
  await createState(storage);
  const originalReplace = storage.replace;
  let bindingWrites = 0;
  let fetchCalls = 0;
  storage.replace = async (path, data) => {
    if (path === "/private/donation.json" && JSON.parse(data).receipt !== null) {
      bindingWrites += 1;
      throw new Error("simulated crash before durable binding");
    }
    return originalReplace.call(storage, path, data);
  };
  await assert.rejects(
    watchDonationCanary({
      statePath: "/private/donation.json",
      receiptOut: "/private/receipt.json",
    }, dependencies({
      storage,
      fetch: async () => {
        fetchCalls += 1;
        return json({});
      },
    })),
    /simulated crash before durable binding/,
  );
  assert.equal(bindingWrites, 1);
  assert.equal(fetchCalls, 0);
  assert.equal(JSON.parse(storage.files.get("/private/donation.json")).receipt, null);
  assert.equal(storage.files.has("/private/receipt.json"), false);
});

test("rate limiting is bounded and fixed to the production donation endpoint", async () => {
  const storage = memoryStorage();
  const sleeps = [];
  let calls = 0;
  await createDonationCanary({
    asset: "btc",
    amountCents: 100,
    statePath: "/private/state",
  }, dependencies({
    storage,
    sleeps,
    fetch: async (url) => {
      calls += 1;
      assert.equal(url, `${API_ORIGIN}/v1/donations/crypto/quote`);
      if (calls < 4) return json({ error: "rate_limited" }, 429, { "retry-after": "2" });
      return json(validQuote("btc", 100));
    },
  }));
  assert.equal(calls, 4);
  assert.deepEqual(sleeps, [2000, 2000, 2000]);
});

test("native files are exclusive, durable mode-0600 regular files", async () => {
  const directory = await mkdtemp(join(tmpdir(), "osl-donation-canary-"));
  const pathname = join(directory, "receipt.json");
  try {
    const storage = createNativeStorage();
    await storage.createExclusive(pathname, "one\n");
    assert.equal((await stat(pathname)).mode & 0o777, 0o600);
    assert.equal(await storage.readSecure(pathname), "one\n");
    await assert.rejects(storage.createExclusive(pathname, "two\n"));
    assert.equal(await readFile(pathname, "utf8"), "one\n");
    const before = await stat(pathname);
    await storage.replace(pathname, "three\n");
    const after = await stat(pathname);
    assert.notEqual(after.ino, before.ino);
    assert.equal(after.mode & 0o777, 0o600);
    assert.equal(await readFile(pathname, "utf8"), "three\n");
    await storage.remove(pathname);
    await assert.rejects(stat(pathname), { code: "ENOENT" });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("CLI accepts only exact command shapes and the inclusive one-dollar to ten-thousand-dollar range", async () => {
  const invalid = [
    ["create", "--asset", "btc", "--amount-cents", "99", "--state", "/x"],
    ["create", "--asset", "btc", "--amount-cents", "1000001", "--state", "/x"],
    ["create", "--asset", "btc", "--amount-cents", "100.5", "--state", "/x"],
    ["create", "--asset", "doge", "--amount-cents", "100", "--state", "/x"],
    ["create", "--asset", "btc", "--amount-cents", "100", "--state", "/x", "--api", "https://evil.test"],
    ["watch", "--state", "/x"],
    ["watch", "--state", "/x", "--receipt-out", "/r", "--asset", "btc"],
  ];
  for (const argv of invalid) {
    await assert.rejects(runCli(argv, dependencies({
      storage: memoryStorage(),
      fetch: async () => json(validQuote()),
    })), /Usage:|asset must be btc or xmr/);
  }

  for (const amountCents of [100, 1_000_000]) {
    const storage = memoryStorage();
    await runCli([
      "create", "--asset", "btc", "--amount-cents", String(amountCents), "--state", "/x",
    ], dependencies({
      storage,
      fetch: async () => json(validQuote("btc", amountCents)),
    }));
    assert.equal(JSON.parse(storage.files.get("/x")).invoice.amount_usd_cents, amountCents);
  }
});

test("client has no entitlement, delivery acknowledgement, wallet, or spending surface", async () => {
  const source = await readFile(new URL("./crypto-donation-live-canary.mjs", import.meta.url), "utf8");
  assert.doesNotMatch(source, /\/v1\/(?:license|subscriptions?)/);
  assert.doesNotMatch(source, /acknowledge_delivery|delivery_public_key|private_jwk/);
  assert.doesNotMatch(source, /wallet[_-]?rpc|sendrawtransaction|transfer|seed phrase|spend key/i);
});
