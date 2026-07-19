import assert from "node:assert/strict";
import { webcrypto } from "node:crypto";
import { mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  API_ORIGIN,
  createCanary,
  createNativeStorage,
  runCli,
  watchCanary,
} from "./crypto-live-canary.mjs";

const NOW = 1_780_000_000;
const LICENSE = "OSL-7K4M-9F3A-8W2P-H6RT";

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

function validQuote(asset = "btc") {
  const amount = asset === "btc" ? "0.00008333" : "0.033333333334";
  return {
    invoice_id: `cpay_${asset === "btc" ? "a" : "b".repeat(32)}`.replace(
      /^cpay_a$/,
      `cpay_${"a".repeat(32)}`,
    ),
    claim_token: "C".repeat(43),
    payment_method: asset,
    address: asset === "btc" ? `bc1q${"q".repeat(38)}` : `8${"1".repeat(94)}`,
    amount_native: amount,
    amount_atomic: asset === "btc" ? "8333" : "33333333334",
    amount_usd_cents: 500,
    price_locked_at: NOW,
    expires_at: NOW + 1800,
    confirmations_required: asset === "btc" ? 2 : 10,
  };
}

function dependencies({ fetch, storage = memoryStorage(), logs = [], now = () => NOW, sleeps = [] }) {
  return {
    fetch,
    storage,
    now,
    sleep: async (milliseconds) => { sleeps.push(milliseconds); },
    timeoutSignal: () => undefined,
    crypto: webcrypto,
    stdout: (line) => logs.push(line),
  };
}

async function encryptedLicenseFromState(
  storage,
  path = "/private/state.json",
  envelopeOverrides = {},
) {
  const state = JSON.parse(storage.files.get(path));
  const publicKey = await webcrypto.subtle.importKey(
    "spki",
    Buffer.from(state.delivery_public_key_spki, "base64"),
    { name: "RSA-OAEP", hash: "SHA-256" },
    false,
    ["encrypt"],
  );
  const encrypted = await webcrypto.subtle.encrypt(
    { name: "RSA-OAEP" },
    publicKey,
    new TextEncoder().encode(JSON.stringify({
      version: 1,
      invoice_id: state.invoice.invoice_id,
      payment_method: state.invoice.payment_method,
      amount_usd_cents: state.invoice.amount_usd_cents,
      plan: "pro",
      activation_code: LICENSE,
      ...envelopeOverrides,
    })),
  );
  return Buffer.from(encrypted).toString("base64");
}

async function createState(storage, logs = []) {
  const requests = [];
  await createCanary({ asset: "btc", statePath: "/private/state.json" }, dependencies({
    storage,
    logs,
    fetch: async (url, init) => {
      requests.push({ url, body: JSON.parse(init.body) });
      return json(validQuote());
    },
  }));
  return requests;
}

test("create generates local RSA delivery state and prints only public invoice details", async () => {
  const storage = memoryStorage();
  const logs = [];
  const requests = await createState(storage, logs);

  assert.equal(requests.length, 1);
  assert.equal(requests[0].url, `${API_ORIGIN}/v1/crypto/quote`);
  assert.deepEqual(Object.keys(requests[0].body).sort(), [
    "delivery_public_key_spki", "payment_method", "plan",
  ]);
  assert.equal(requests[0].body.plan, "pro");
  assert.equal(requests[0].body.payment_method, "btc");

  const stateText = storage.files.get("/private/state.json");
  const state = JSON.parse(stateText);
  assert.equal(state.invoice.claim_token, "C".repeat(43));
  assert.equal(state.private_jwk.kty, "RSA");
  assert.ok(typeof state.private_jwk.d === "string" && state.private_jwk.d.length > 100);
  assert.match(logs.join("\n"), /Invoice: cpay_/);
  assert.match(logs.join("\n"), /Address: bc1q/);
  assert.doesNotMatch(logs.join("\n"), new RegExp(state.invoice.claim_token));
  assert.doesNotMatch(logs.join("\n"), new RegExp(state.private_jwk.d));
});

test("create rejects unexpected invoice fields and removes sensitive provisional state", async () => {
  const storage = memoryStorage();
  const logs = [];
  await assert.rejects(
    createCanary({ asset: "btc", statePath: "/private/state.json" }, dependencies({
      storage,
      logs,
      fetch: async () => json({ ...validQuote(), attacker_field: "accepted" }),
    })),
    /invoice response schema is invalid/,
  );
  assert.equal(storage.files.has("/private/state.json"), false);
  assert.equal(storage.files.size, 0);
  assert.deepEqual(logs, []);
});

test("watch decrypts, validates ACTIVE lifetime Pro, then and only then acknowledges and clears state", async () => {
  const storage = memoryStorage();
  const logs = [];
  await createState(storage, logs);
  logs.length = 0;
  const encrypted = await encryptedLicenseFromState(storage);
  const calls = [];
  const fetch = async (url, init) => {
    const body = JSON.parse(init.body);
    calls.push({ url, body });
    if (url.endsWith("/v1/license/validate")) {
      assert.equal(body.license_key, LICENSE);
      return json({ status: "ACTIVE", current_period_end: null, checksum_ok: true });
    }
    if (body.acknowledge_delivery === true) {
      return json({ status: "acknowledged", already_acknowledged: false });
    }
    return json({
      invoice_id: validQuote().invoice_id,
      status: "delivery_ready",
      expires_at: validQuote().expires_at,
      encrypted_license: encrypted,
      delivery: "rsa-oaep-sha256",
    });
  };

  const result = await runCli([
    "watch",
    "--state", "/private/state.json",
    "--activation-out", "/private/activation.txt",
  ], dependencies({
    storage,
    logs,
    fetch,
    // Resume well after the quote freshness window, but before invoice expiry.
    now: () => NOW + 20 * 60,
  }));
  assert.deepEqual(result, { status: "acknowledged" });
  assert.deepEqual(calls.map((call) => [new URL(call.url).pathname, call.body.acknowledge_delivery === true]), [
    ["/v1/crypto/status", false],
    ["/v1/license/validate", false],
    ["/v1/crypto/status", true],
  ]);
  assert.equal(storage.files.has("/private/state.json"), false);
  assert.equal(storage.files.size, 1);
  assert.equal(storage.files.get("/private/activation.txt"), `${LICENSE}\n`);
  assert.equal(storage.modes.get("/private/activation.txt"), 0o600);
  assert.doesNotMatch(logs.join("\n"), new RegExp(LICENSE));
  assert.doesNotMatch(logs.join("\n"), /claim_token|private_jwk|encrypted_license/);
});

test("watch never acknowledges when RSA delivery cannot be decrypted", async () => {
  const storage = memoryStorage();
  await createState(storage);
  const calls = [];
  const result = watchCanary({
    statePath: "/private/state.json",
    activationOut: "/private/activation.txt",
  }, dependencies({
    storage,
    fetch: async (url, init) => {
      calls.push({ url, body: JSON.parse(init.body) });
      return json({
        invoice_id: validQuote().invoice_id,
        status: "delivery_ready",
        expires_at: validQuote().expires_at,
        encrypted_license: Buffer.alloc(256, 7).toString("base64"),
        delivery: "rsa-oaep-sha256",
      });
    },
  }));
  await assert.rejects(result, /could not be decrypted/);
  assert.equal(calls.length, 1);
  assert.equal(calls.some((call) => call.body.acknowledge_delivery === true), false);
  assert.equal(storage.files.has("/private/state.json"), true);
});

test("watch rejects a cryptographically valid envelope swapped from another invoice before output or ack", async () => {
  const storage = memoryStorage();
  const logs = [];
  await createState(storage);
  const swapped = await encryptedLicenseFromState(storage, "/private/state.json", {
    invoice_id: `cpay_${"d".repeat(32)}`,
  });
  const calls = [];
  await assert.rejects(
    watchCanary({
      statePath: "/private/state.json",
      activationOut: "/private/activation.txt",
    }, dependencies({
      storage,
      logs,
      fetch: async (url, init) => {
        calls.push({ url, body: JSON.parse(init.body) });
        return json({
          invoice_id: validQuote().invoice_id,
          status: "delivery_ready",
          expires_at: validQuote().expires_at,
          encrypted_license: swapped,
          delivery: "rsa-oaep-sha256",
        });
      },
    })),
    /could not be decrypted or bound to this invoice/,
  );
  assert.equal(calls.length, 1);
  assert.equal(new URL(calls[0].url).pathname, "/v1/crypto/status");
  assert.equal(calls.some((call) => call.body.acknowledge_delivery === true), false);
  assert.equal(storage.files.has("/private/activation.txt"), false);
  assert.equal(storage.files.has("/private/state.json"), true);
  assert.doesNotMatch(logs.join("\n"), new RegExp(LICENSE));
});

test("watch never acknowledges when public activation validation is not ACTIVE", async () => {
  const storage = memoryStorage();
  await createState(storage);
  const encrypted = await encryptedLicenseFromState(storage);
  const calls = [];
  await assert.rejects(
    watchCanary({
      statePath: "/private/state.json",
      activationOut: "/private/activation.txt",
    }, dependencies({
      storage,
      fetch: async (url, init) => {
        const body = JSON.parse(init.body);
        calls.push({ url, body });
        if (url.endsWith("/v1/license/validate")) {
          return json({ status: "UNKNOWN", current_period_end: null, checksum_ok: true });
        }
        return json({
          invoice_id: validQuote().invoice_id,
          status: "delivery_ready",
          expires_at: validQuote().expires_at,
          encrypted_license: encrypted,
          delivery: "rsa-oaep-sha256",
        });
      },
    })),
    /did not validate as lifetime Pro/,
  );
  assert.deepEqual(calls.map((call) => new URL(call.url).pathname), [
    "/v1/crypto/status", "/v1/license/validate",
  ]);
  assert.equal(calls.some((call) => call.body.acknowledge_delivery === true), false);
  assert.equal(storage.files.has("/private/state.json"), true);
});

test("watch fails closed on extra status fields before decryption or acknowledgement", async () => {
  const storage = memoryStorage();
  await createState(storage);
  const encrypted = await encryptedLicenseFromState(storage);
  let calls = 0;
  await assert.rejects(
    watchCanary({
      statePath: "/private/state.json",
      activationOut: "/private/activation.txt",
    }, dependencies({
      storage,
      fetch: async () => {
        calls += 1;
        return json({
          invoice_id: validQuote().invoice_id,
          status: "delivery_ready",
          expires_at: validQuote().expires_at,
          encrypted_license: encrypted,
          delivery: "rsa-oaep-sha256",
          extra: true,
        });
      },
    })),
    /status response schema is invalid/,
  );
  assert.equal(calls, 1);
  assert.equal(storage.files.has("/private/state.json"), true);
});

test("rate limits are bounded, honor Retry-After, and retain fixed API endpoints", async () => {
  const storage = memoryStorage();
  const sleeps = [];
  let calls = 0;
  await createCanary({ asset: "xmr", statePath: "/private/xmr.json" }, dependencies({
    storage,
    sleeps,
    fetch: async (url) => {
      calls += 1;
      assert.equal(url, `${API_ORIGIN}/v1/crypto/quote`);
      if (calls < 3) return json({ error: "rate_limited" }, 429, { "retry-after": "2" });
      return json(validQuote("xmr"));
    },
  }));
  assert.equal(calls, 3);
  assert.deepEqual(sleeps, [2000, 2000]);
});

test("watch never acknowledges when durable activation output creation fails", async () => {
  const storage = memoryStorage();
  await createState(storage);
  const encrypted = await encryptedLicenseFromState(storage);
  const calls = [];
  const originalCreate = storage.createExclusive;
  storage.createExclusive = async (path, data) => {
    if (path === "/private/activation.txt") throw new Error("simulated disk failure");
    return originalCreate.call(storage, path, data);
  };
  await assert.rejects(
    watchCanary({
      statePath: "/private/state.json",
      activationOut: "/private/activation.txt",
    }, dependencies({
      storage,
      fetch: async (url, init) => {
        const body = JSON.parse(init.body);
        calls.push({ url, body });
        if (url.endsWith("/v1/license/validate")) {
          return json({ status: "ACTIVE", current_period_end: null, checksum_ok: true });
        }
        return json({
          invoice_id: validQuote().invoice_id,
          status: "delivery_ready",
          expires_at: validQuote().expires_at,
          encrypted_license: encrypted,
          delivery: "rsa-oaep-sha256",
        });
      },
    })),
    /activation output could not be written securely/,
  );
  assert.equal(calls.some((call) => call.body.acknowledge_delivery === true), false);
  assert.equal(storage.files.has("/private/state.json"), true);
  assert.equal(storage.files.has("/private/activation.txt"), false);
});

test("lost acknowledgement response resumes from durable binding and cleans state on exact 410", async () => {
  const storage = memoryStorage();
  const logs = [];
  await createState(storage);
  const encrypted = await encryptedLicenseFromState(storage);
  const calls = [];
  let acknowledgementProcessed = false;
  const firstFetch = async (url, init) => {
    const body = JSON.parse(init.body);
    calls.push({ attempt: 1, url, body });
    if (url.endsWith("/v1/license/validate")) {
      return json({ status: "ACTIVE", current_period_end: null, checksum_ok: true });
    }
    if (body.acknowledge_delivery === true) {
      acknowledgementProcessed = true;
      throw new Error("response lost after server commit");
    }
    return json({
      invoice_id: validQuote().invoice_id,
      status: "delivery_ready",
      expires_at: validQuote().expires_at,
      encrypted_license: encrypted,
      delivery: "rsa-oaep-sha256",
    });
  };
  await assert.rejects(
    watchCanary({
      statePath: "/private/state.json",
      activationOut: "/private/activation.txt",
    }, dependencies({ storage, logs, fetch: firstFetch })),
    /API request failed/,
  );
  assert.equal(acknowledgementProcessed, true);
  assert.equal(storage.files.get("/private/activation.txt"), `${LICENSE}\n`);
  const durableState = JSON.parse(storage.files.get("/private/state.json"));
  assert.deepEqual(Object.keys(durableState.activation).sort(), ["content_sha256", "output_path"]);
  assert.equal(durableState.activation.output_path, "/private/activation.txt");
  assert.match(durableState.activation.content_sha256, /^[0-9a-f]{64}$/);

  const secondFetch = async (url, init) => {
    const body = JSON.parse(init.body);
    calls.push({ attempt: 2, url, body });
    if (url.endsWith("/v1/license/validate")) {
      assert.equal(body.license_key, LICENSE);
      return json({ status: "ACTIVE", current_period_end: null, checksum_ok: true });
    }
    assert.equal(body.acknowledge_delivery, undefined);
    return json({ error: "crypto delivery already acknowledged" }, 410);
  };
  const resumed = await watchCanary({
    statePath: "/private/state.json",
    activationOut: "/private/activation.txt",
  }, dependencies({ storage, logs, fetch: secondFetch }));
  assert.deepEqual(resumed, { status: "acknowledged" });
  assert.equal(storage.files.has("/private/state.json"), false);
  assert.equal(storage.files.get("/private/activation.txt"), `${LICENSE}\n`);
  assert.equal(calls.filter((call) => call.body.acknowledge_delivery === true).length, 1);
  assert.equal(calls.some((call) => new URL(call.url).pathname === "/v1/crypto/quote"), false);
  assert.doesNotMatch(logs.join("\n"), new RegExp(LICENSE));
});

test("expired invoices recover reconciled delivery on a manual retry after the 24h grace boundary", async () => {
  const storage = memoryStorage();
  const logs = [];
  await createState(storage);
  const expired = await watchCanary({
    statePath: "/private/state.json",
    activationOut: "/private/activation.txt",
  }, dependencies({
    storage,
    logs,
    fetch: async () => json({
      invoice_id: validQuote().invoice_id,
      status: "expired",
      expires_at: validQuote().expires_at,
      encrypted_license: null,
      delivery: null,
    }),
  }));
  assert.deepEqual(expired, { status: "expired", reconciliation_required: true });
  assert.equal(storage.files.has("/private/state.json"), true);
  assert.match(logs.join("\n"), /reconciliation required/);

  const encrypted = await encryptedLicenseFromState(storage);
  const recovered = await watchCanary({
    statePath: "/private/state.json",
    activationOut: "/private/activation.txt",
  }, dependencies({
    storage,
    logs,
    // Manual reconciliation must still make one status request even after the
    // automatic confirmation-polling grace deadline has elapsed.
    now: () => NOW + 1800 + (24 * 60 * 60) + 1,
    fetch: async (url, init) => {
      const body = JSON.parse(init.body);
      if (url.endsWith("/v1/license/validate")) {
        return json({ status: "ACTIVE", current_period_end: null, checksum_ok: true });
      }
      if (body.acknowledge_delivery === true) {
        return json({ status: "acknowledged", already_acknowledged: false });
      }
      return json({
        invoice_id: validQuote().invoice_id,
        status: "delivery_ready",
        expires_at: validQuote().expires_at,
        encrypted_license: encrypted,
        delivery: "rsa-oaep-sha256",
      });
    },
  }));
  assert.deepEqual(recovered, { status: "acknowledged" });
  assert.equal(storage.files.has("/private/state.json"), false);
  assert.equal(storage.files.get("/private/activation.txt"), `${LICENSE}\n`);
});

test("native secret output creation is exclusive and mode 0600", async () => {
  const directory = await mkdtemp(join(tmpdir(), "osl-canary-test-"));
  const pathname = join(directory, "activation.txt");
  try {
    const storage = createNativeStorage();
    await storage.createExclusive(pathname, `${LICENSE}\n`);
    const metadata = await stat(pathname);
    assert.equal(metadata.mode & 0o777, 0o600);
    assert.equal(await storage.readSecure(pathname), `${LICENSE}\n`);
    await assert.rejects(storage.createExclusive(pathname, "replacement\n"));
    assert.equal(await storage.readSecure(pathname), `${LICENSE}\n`);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("native create fsyncs the parent and removes an uncommitted entry when directory sync fails", async () => {
  const directory = await mkdtemp(join(tmpdir(), "osl-canary-create-sync-"));
  const pathname = join(directory, "secret.txt");
  let syncAttempts = 0;
  try {
    const storage = createNativeStorage({
      syncDirectory: async () => {
        syncAttempts += 1;
        throw new Error("simulated directory fsync failure");
      },
    });
    await assert.rejects(storage.createExclusive(pathname, "secret\n"), /directory fsync failure/);
    assert.ok(syncAttempts >= 2);
    await assert.rejects(stat(pathname), { code: "ENOENT" });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("atomic replace keeps the old complete file and cleans its temp when rename fails", async () => {
  const directory = await mkdtemp(join(tmpdir(), "osl-canary-rename-fail-"));
  const pathname = join(directory, "state.json");
  try {
    await writeFile(pathname, "old-complete\n", { mode: 0o600 });
    const storage = createNativeStorage({
      rename: async () => { throw new Error("simulated rename failure"); },
      randomToken: () => "rename-failure",
    });
    await assert.rejects(storage.replace(pathname, "new-complete\n"), /rename failure/);
    assert.equal(await readFile(pathname, "utf8"), "old-complete\n");
    assert.deepEqual(await readdir(directory), ["state.json"]);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("atomic replace exposes only the complete new file if parent fsync fails after rename", async () => {
  const directory = await mkdtemp(join(tmpdir(), "osl-canary-post-rename-"));
  const pathname = join(directory, "state.json");
  try {
    await writeFile(pathname, "old-complete\n", { mode: 0o600 });
    const storage = createNativeStorage({
      syncDirectory: async () => { throw new Error("simulated post-rename fsync failure"); },
      randomToken: () => "post-rename-failure",
    });
    await assert.rejects(storage.replace(pathname, "new-complete\n"), /post-rename fsync failure/);
    assert.equal(await readFile(pathname, "utf8"), "new-complete\n");
    assert.deepEqual(await readdir(directory), ["state.json"]);
    assert.equal((await stat(pathname)).mode & 0o777, 0o600);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("CLI accepts only the two explicit command shapes", async () => {
  const storage = memoryStorage();
  await assert.rejects(
    runCli(["create", "--asset", "btc", "--state", "/x", "--api", "https://evil.test"],
      dependencies({ storage, fetch: async () => json(validQuote()) })),
    /Usage:/,
  );
  await assert.rejects(
    runCli(["watch", "--asset", "btc", "--state", "/x"], dependencies({
      storage,
      fetch: async () => json({}),
    })),
    /Usage:/,
  );
  await assert.rejects(
    runCli(["watch", "--state", "/x"], dependencies({ storage, fetch: async () => json({}) })),
    /Usage:/,
  );
  await assert.rejects(
    runCli(["create", "--asset", "btc", "--state", "/x", "--activation-out", "/y"],
      dependencies({ storage, fetch: async () => json(validQuote()) })),
    /Usage:/,
  );
});
