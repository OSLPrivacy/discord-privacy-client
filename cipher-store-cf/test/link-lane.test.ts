import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  handleLinkBurn,
  handleLinkCreate,
  handleLinkFetch,
  handleLinkRevoke,
  handleLinkStatus,
  LINK_TTL_SECONDS,
  MAX_LINK_BYTES,
  RESERVATION_SECONDS,
} from "../src/endpoints/link.js";
import { sweepExpiredLinks } from "../src/lib/sweep.js";
import { rateLimit } from "../src/lib/rate-limit.js";
import { migratedD1 } from "./helpers/d1.js";
import { sha256Hex } from "../src/lib/digest.js";
import {
  GRANT_AUDIENCE,
  GRANT_DOMAIN,
  GRANT_SCHEME,
} from "../src/lib/link-grant.js";

const ORIGIN = "https://links.test";

interface Row {
  id: string;
  data: Uint8Array | null;
  size_bytes: number;
  created_at: number;
  expires_at: number;
  fetch_token_sha256_hex: string;
  manage_token_sha256_hex: string;
  retrieved_at: number | null;
  retrieval_count: number;
  reserved_until: number | null;
  burned_at: number | null;
}

function linkDb() {
  const db = migratedD1();
  const kv = new Map<string, string>();
  const env = {
    DB: db.d1,
    RATE_LIMIT: {
      get: async (k: string) => kv.get(k) ?? null,
      put: async (k: string, val: string) => void kv.set(k, val),
    },
    RATE_LIMIT_HASH_KEY: "k".repeat(48),
  } as unknown as Env;
  return { env, db, kv };
}

type LinkDbState = ReturnType<typeof linkDb>;

function rowFor(state: LinkDbState, id: string): Row {
  const row = state.db.raw
    .prepare("SELECT * FROM view_once_links WHERE id = ?")
    .get(id) as Row | undefined;
  if (!row) throw new Error(`missing link row ${id}`);
  return row;
}

function linkCount(state: LinkDbState): number {
  return state.db.count("SELECT COUNT(*) AS c FROM view_once_links");
}

// ---- Grant issuance (stands in for the keyserver) ---------------------

async function issuer() {
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, [
    "sign",
    "verify",
  ])) as CryptoKeyPair;
  const rawPub = new Uint8Array(
    await crypto.subtle.exportKey("raw", pair.publicKey) as ArrayBuffer,
  );
  let bin = "";
  for (const b of rawPub) bin += String.fromCharCode(b);
  return { pair, pubB64: btoa(bin) };
}

async function grantHeader(
  pair: CryptoKeyPair,
  overrides: { exp?: number; aud?: string; jti?: string } = {},
): Promise<string> {
  const now = Math.floor(Date.now() / 1000);
  const jti =
    overrides.jti ??
    [...crypto.getRandomValues(new Uint8Array(16))]
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
  const payload = new TextEncoder().encode(
    JSON.stringify({
      aud: overrides.aud ?? GRANT_AUDIENCE,
      exp: overrides.exp ?? now + 120,
      jti,
    }),
  );
  const domain = new TextEncoder().encode(GRANT_DOMAIN);
  const message = new Uint8Array(domain.byteLength + 1 + payload.byteLength);
  message.set(domain, 0);
  message[domain.byteLength] = 0;
  message.set(payload, domain.byteLength + 1);
  const sig = new Uint8Array(
    await crypto.subtle.sign({ name: "Ed25519" }, pair.privateKey, message),
  );
  const b64u = (bytes: Uint8Array) => {
    let s = "";
    for (const b of bytes) s += String.fromCharCode(b);
    return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  };
  return `${GRANT_SCHEME} ${b64u(payload)}.${b64u(sig)}`;
}

// ---- Helpers ----------------------------------------------------------

const FETCH_TOKEN = "0123456789abcdef0123456789abcdef";
const MANAGE_TOKEN = "fedcba9876543210fedcba9876543210";

function sealedBody(size = 64): Uint8Array {
  const bytes = new Uint8Array(size);
  crypto.getRandomValues(bytes);
  return bytes;
}

async function createLink(
  env: Env,
  auth: string,
  body: Uint8Array = sealedBody(),
): Promise<{ id: string; expires_at: number }> {
  const res = await handleLinkCreate(
    new Request(ORIGIN + "/v1/link", {
      method: "POST",
      headers: {
        authorization: auth,
        "x-osl-ttl-seconds": String(LINK_TTL_SECONDS),
        "x-osl-fetch-token": FETCH_TOKEN,
        "x-osl-manage-token": MANAGE_TOKEN,
      },
      body,
    }),
    env,
  );
  expect(res.status).toBe(201);
  return (await res.json()) as { id: string; expires_at: number };
}

function fetchRequest(
  id: string,
  token: string,
  overrides: { origin?: string | null; gesture?: string | null } = {},
): Request {
  const headers: Record<string, string> = { "content-type": "application/json" };
  const origin = overrides.origin === undefined ? ORIGIN : overrides.origin;
  if (origin !== null) headers["origin"] = origin;
  const gesture = overrides.gesture === undefined ? "1" : overrides.gesture;
  if (gesture !== null) headers["x-osl-gesture"] = gesture;
  return new Request(`${ORIGIN}/v/${id}/fetch`, {
    method: "POST",
    headers,
    body: JSON.stringify({ t: token }),
  });
}

// ---- Tests ------------------------------------------------------------

describe("view-once link creation", () => {
  it("stores opaque ciphertext and only capability digests", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const body = sealedBody(128);
    const { id, expires_at } = await createLink(
      state.env,
      await grantHeader(iss.pair),
      body,
    );

    expect(id).toMatch(/^[0-9a-f]{32}$/);
    const row = rowFor(state, id);
    expect(row.data).toEqual(body);
    expect(expires_at - row.created_at).toBe(LINK_TTL_SECONDS);
    // Neither bearer capability is stored, only its digest.
    const dump = JSON.stringify(row, (_k, v) =>
      v instanceof Uint8Array ? "bytes" : v,
    );
    expect(dump).not.toContain(FETCH_TOKEN);
    expect(dump).not.toContain(MANAGE_TOKEN);
    expect(row.fetch_token_sha256_hex).toBe(await sha256Hex(FETCH_TOKEN));
    expect(row.manage_token_sha256_hex).toBe(await sha256Hex(MANAGE_TOKEN));
    // No column exists that could hold a key, an IP or a user agent.
    expect(Object.keys(row).sort()).toEqual([
      "burned_at",
      "created_at",
      "data",
      "expires_at",
      "fetch_token_sha256_hex",
      "id",
      "manage_token_sha256_hex",
      "reserved_until",
      "retrieval_count",
      "retrieved_at",
      "size_bytes",
    ]);
  });

  it("refuses creation outright when no grant issuer is configured", async () => {
    const state = linkDb();
    const res = await handleLinkCreate(
      new Request(ORIGIN + "/v1/link", {
        method: "POST",
        headers: {
          "x-osl-ttl-seconds": String(LINK_TTL_SECONDS),
          "x-osl-fetch-token": FETCH_TOKEN,
          "x-osl-manage-token": MANAGE_TOKEN,
        },
        body: sealedBody(),
      }),
      state.env,
    );
    // Fail closed: an unconfigured verifier must never become an open,
    // logless, self-deleting file host.
    expect(res.status).toBe(503);
    expect(await res.json()).toMatchObject({ error: "link_creation_unconfigured" });
    expect(linkCount(state)).toBe(0);
  });

  it("accepts only the 1-hour TTL the sender warning promises", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    for (const ttl of ["86400", "600", "3601", "03600"]) {
      const res = await handleLinkCreate(
        new Request(ORIGIN + "/v1/link", {
          method: "POST",
          headers: {
            authorization: await grantHeader(iss.pair),
            "x-osl-ttl-seconds": ttl,
            "x-osl-fetch-token": FETCH_TOKEN,
            "x-osl-manage-token": MANAGE_TOKEN,
          },
          body: sealedBody(),
        }),
        state.env,
      );
      expect(res.status).toBe(400);
      expect(await res.json()).toMatchObject({ error: "bad_ttl" });
    }
    expect(linkCount(state)).toBe(0);
  });

  it("rejects an oversized or degenerate payload", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const big = await handleLinkCreate(
      new Request(ORIGIN + "/v1/link", {
        method: "POST",
        headers: {
          authorization: await grantHeader(iss.pair),
          "x-osl-ttl-seconds": String(LINK_TTL_SECONDS),
          "x-osl-fetch-token": FETCH_TOKEN,
          "x-osl-manage-token": MANAGE_TOKEN,
          "content-length": String(MAX_LINK_BYTES + 1),
        },
        body: sealedBody(),
      }),
      state.env,
    );
    expect(big.status).toBe(413);

    const tiny = await handleLinkCreate(
      new Request(ORIGIN + "/v1/link", {
        method: "POST",
        headers: {
          authorization: await grantHeader(iss.pair),
          "x-osl-ttl-seconds": String(LINK_TTL_SECONDS),
          "x-osl-fetch-token": FETCH_TOKEN,
          "x-osl-manage-token": MANAGE_TOKEN,
        },
        body: sealedBody(8),
      }),
      state.env,
    );
    expect(tiny.status).toBe(400);
    expect(linkCount(state)).toBe(0);
  });

  it("requires the two capabilities to be distinct", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const res = await handleLinkCreate(
      new Request(ORIGIN + "/v1/link", {
        method: "POST",
        headers: {
          authorization: await grantHeader(iss.pair),
          "x-osl-ttl-seconds": String(LINK_TTL_SECONDS),
          "x-osl-fetch-token": FETCH_TOKEN,
          "x-osl-manage-token": FETCH_TOKEN,
        },
        body: sealedBody(),
      }),
      state.env,
    );
    expect(res.status).toBe(400);
  });
});

describe("a crawler cannot burn a view", () => {
  it("does not release ciphertext without the trusted-gesture header", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const { id } = await createLink(state.env, await grantHeader(iss.pair));

    const res = await handleLinkFetch(
      fetchRequest(id, FETCH_TOKEN, { gesture: null }),
      state.env,
      id,
    );
    expect(res.status).toBe(404);
    // Crucially, nothing was consumed: the link is untouched.
    const row = rowFor(state, id);
    expect(row.data).not.toBeNull();
    expect(row.reserved_until).toBeNull();
    expect(row.retrieved_at).toBeNull();
    expect(row.retrieval_count).toBe(0);
  });

  it("does not release ciphertext to a cross-origin or origin-less caller", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const { id } = await createLink(state.env, await grantHeader(iss.pair));

    for (const origin of [null, "https://evil.test", "https://links.test.evil"]) {
      const res = await handleLinkFetch(
        fetchRequest(id, FETCH_TOKEN, { origin }),
        state.env,
        id,
      );
      expect(res.status).toBe(404);
    }
    expect(rowFor(state, id).reserved_until).toBeNull();
  });

  it("does not release ciphertext to a wrong token, and says nothing about why", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const { id } = await createLink(state.env, await grantHeader(iss.pair));

    const wrong = await handleLinkFetch(
      fetchRequest(id, "f".repeat(32)),
      state.env,
      id,
    );
    const missing = await handleLinkFetch(
      fetchRequest("a".repeat(32), FETCH_TOKEN),
      state.env,
      "a".repeat(32),
    );
    expect(wrong.status).toBe(missing.status);
    expect(await wrong.json()).toEqual(await missing.json());
    expect(rowFor(state, id).retrieval_count).toBe(0);
  });
});

describe("the burn is not on fetch", () => {
  it("reserves for 60 seconds and serves repeat fetches idempotently", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const body = sealedBody(96);
    const { id } = await createLink(state.env, await grantHeader(iss.pair), body);

    const first = await handleLinkFetch(fetchRequest(id, FETCH_TOKEN), state.env, id);
    expect(first.status).toBe(200);
    expect(new Uint8Array(await first.arrayBuffer())).toEqual(body);
    expect(first.headers.get("cache-control")).toBe("no-store");

    const row = rowFor(state, id);
    expect(row.retrieved_at).not.toBeNull();
    expect(row.reserved_until).toBe(row.retrieved_at! + RESERVATION_SECONDS);
    expect(row.retrieval_count).toBe(1);

    // A flaky network must not destroy content the recipient never saw.
    const second = await handleLinkFetch(fetchRequest(id, FETCH_TOKEN), state.env, id);
    expect(second.status).toBe(200);
    expect(new Uint8Array(await second.arrayBuffer())).toEqual(body);
    expect(rowFor(state, id).retrieval_count).toBe(2);
  });

  it("refuses once the reservation window has closed, even before the sweep", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const { id } = await createLink(state.env, await grantHeader(iss.pair));
    await handleLinkFetch(fetchRequest(id, FETCH_TOKEN), state.env, id);

    // Wind the reservation into the past without running the sweep.
    state.db.exec(
      "UPDATE view_once_links SET reserved_until = ? WHERE id = ?",
      Math.floor(Date.now() / 1000) - 1,
      id,
    );
    const res = await handleLinkFetch(fetchRequest(id, FETCH_TOKEN), state.env, id);
    expect(res.status).toBe(404);
    // Destroyed on sight rather than waiting for the cron.
    expect(rowFor(state, id).data).toBeNull();
    expect(rowFor(state, id).burned_at).not.toBeNull();
  });

  it("is destroyed by the sweep regardless of any client confirmation", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const retrieved = await createLink(state.env, await grantHeader(iss.pair));
    const untouched = await createLink(state.env, await grantHeader(iss.pair));
    await handleLinkFetch(fetchRequest(retrieved.id, FETCH_TOKEN), state.env, retrieved.id);

    const now = Math.floor(Date.now() / 1000);
    // Reservation closed for one; TTL elapsed for the other.
    state.db.exec(
      "UPDATE view_once_links SET reserved_until = ? WHERE id = ?",
      now - 1,
      retrieved.id,
    );
    state.db.exec(
      "UPDATE view_once_links SET expires_at = ? WHERE id = ?",
      now - 1,
      untouched.id,
    );

    expect(await sweepExpiredLinks(state.env)).toBe(2);
    expect(rowFor(state, retrieved.id).data).toBeNull();
    expect(rowFor(state, untouched.id).data).toBeNull();
  });

  it("keeps a content-free receipt, then purges it a day after expiry", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const { id } = await createLink(state.env, await grantHeader(iss.pair));
    await handleLinkFetch(fetchRequest(id, FETCH_TOKEN), state.env, id);
    state.db.exec(
      "UPDATE view_once_links SET reserved_until = ? WHERE id = ?",
      Math.floor(Date.now() / 1000) - 1,
      id,
    );
    await sweepExpiredLinks(state.env);

    const receipt = rowFor(state, id);
    expect(receipt.data).toBeNull();
    expect(receipt.size_bytes).toBe(0);
    expect(receipt.retrieved_at).not.toBeNull();

    state.db.exec(
      "UPDATE view_once_links SET expires_at = ? WHERE id = ?",
      Math.floor(Date.now() / 1000) - 25 * 60 * 60,
      id,
    );
    await sweepExpiredLinks(state.env);
    expect(linkCount(state)).toBe(0);
  });

  it("lets the page burn early, and ignores a burn it cannot authenticate", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const { id } = await createLink(state.env, await grantHeader(iss.pair));
    await handleLinkFetch(fetchRequest(id, FETCH_TOKEN), state.env, id);

    const burnRequest = (token: string, origin: string | null = ORIGIN) => {
      const headers: Record<string, string> = { "content-type": "application/json" };
      if (origin !== null) headers["origin"] = origin;
      return new Request(`${ORIGIN}/v/${id}/burn`, {
        method: "POST",
        headers,
        body: JSON.stringify({ t: token }),
      });
    };

    const wrong = await handleLinkBurn(burnRequest("e".repeat(32)), state.env, id);
    expect(wrong.status).toBe(204);
    expect(rowFor(state, id).data).not.toBeNull();

    const crossOrigin = await handleLinkBurn(
      burnRequest(FETCH_TOKEN, "https://evil.test"),
      state.env,
      id,
    );
    expect(crossOrigin.status).toBe(204);
    expect(rowFor(state, id).data).not.toBeNull();

    const real = await handleLinkBurn(burnRequest(FETCH_TOKEN), state.env, id);
    expect(real.status).toBe(204);
    expect(rowFor(state, id).data).toBeNull();
  });
});

describe("sender-facing status", () => {
  it("reports created, then retrieved, and never says read/seen/viewed", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const { id } = await createLink(state.env, await grantHeader(iss.pair));

    const statusRequest = (token: string) =>
      new Request(`${ORIGIN}/v1/link/${id}/status`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ t: token }),
      });

    const created = await handleLinkStatus(statusRequest(MANAGE_TOKEN), state.env, id);
    expect(created.status).toBe(200);
    const createdBody = (await created.json()) as { state: string };
    expect(createdBody.state).toBe("created");

    await handleLinkFetch(fetchRequest(id, FETCH_TOKEN), state.env, id);
    const after = await handleLinkStatus(statusRequest(MANAGE_TOKEN), state.env, id);
    const afterBody = (await after.json()) as {
      state: string;
      retrieved_at: number;
      retrieval_count: number;
    };
    expect(afterBody.state).toBe("retrieved");
    expect(Number.isInteger(afterBody.retrieved_at)).toBe(true);
    expect(afterBody.retrieval_count).toBe(1);
    for (const banned of ["read", "seen", "viewed"]) {
      expect(JSON.stringify(afterBody)).not.toContain(banned);
    }
  });

  it("reports expired when the hour elapsed with no retrieval", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const { id } = await createLink(state.env, await grantHeader(iss.pair));
    state.db.exec(
      "UPDATE view_once_links SET expires_at = ? WHERE id = ?",
      Math.floor(Date.now() / 1000) - 1,
      id,
    );
    await sweepExpiredLinks(state.env);

    const res = await handleLinkStatus(
      new Request(`${ORIGIN}/v1/link/${id}/status`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ t: MANAGE_TOKEN }),
      }),
      state.env,
      id,
    );
    expect((await res.json()) as { state: string }).toMatchObject({ state: "expired" });
  });

  it("does not answer status or revoke to the recipient's capability", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const { id } = await createLink(state.env, await grantHeader(iss.pair));

    const withToken = (path: string, token: string) =>
      new Request(`${ORIGIN}${path}`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ t: token }),
      });

    const status = await handleLinkStatus(
      withToken(`/v1/link/${id}/status`, FETCH_TOKEN),
      state.env,
      id,
    );
    expect(status.status).toBe(404);

    const revoke = await handleLinkRevoke(
      withToken(`/v1/link/${id}`, FETCH_TOKEN),
      state.env,
      id,
    );
    expect(revoke.status).toBe(404);
    expect(rowFor(state, id).data).not.toBeNull();
  });

  it("lets the sender revoke before anyone retrieves", async () => {
    const state = linkDb();
    const iss = await issuer();
    state.env.LINK_GRANT_PUBKEY_B64 = iss.pubB64;
    const { id } = await createLink(state.env, await grantHeader(iss.pair));
    const res = await handleLinkRevoke(
      new Request(`${ORIGIN}/v1/link/${id}`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ t: MANAGE_TOKEN }),
      }),
      state.env,
      id,
    );
    expect(res.status).toBe(204);
    expect(rowFor(state, id).data).toBeNull();
    const after = await handleLinkFetch(fetchRequest(id, FETCH_TOKEN), state.env, id);
    expect(after.status).toBe(404);
  });
});

describe("link rate-limit buckets", () => {
  /// `link-create` is a mutation bucket, so it is counted atomically in D1;
  /// `link-fetch` stays on KV and fails open. Both stores are provided here.
  function limiterEnv() {
    const kv = new Map<string, string>();
    const db = migratedD1();
    return {
      DB: db.d1,
      RATE_LIMIT: {
        get: async (k: string) => kv.get(k) ?? null,
        put: async (k: string, v: string) => void kv.set(k, v),
      },
      RATE_LIMIT_HASH_KEY: "k".repeat(48),
    } as unknown as Env;
  }

  it("caps link retrieval at 120/hr, far below the 3600/hr blob fetch budget", async () => {
    const env = limiterEnv();
    for (let i = 0; i < 120; i++) {
      const rl = await rateLimit(env, "203.0.113.9", "link-fetch");
      expect(rl.allowed).toBe(true);
    }
    expect(await rateLimit(env, "203.0.113.9", "link-fetch")).toEqual({
      allowed: false,
      remaining: 0,
    });
    // The generic blob bucket is untouched and still generous.
    expect((await rateLimit(env, "203.0.113.9", "fetch")).allowed).toBe(true);
  });

  it("caps link creation at 120/hr on its own bucket", async () => {
    const env = limiterEnv();
    for (let i = 0; i < 120; i++) {
      expect((await rateLimit(env, "203.0.113.10", "link-create")).allowed).toBe(true);
    }
    expect((await rateLimit(env, "203.0.113.10", "link-create")).allowed).toBe(false);
    // Buckets are independent.
    expect((await rateLimit(env, "203.0.113.10", "link-fetch")).allowed).toBe(true);
  });

  it("denies creation but keeps retrieval available when the limiter is down", async () => {
    const env = {
      RATE_LIMIT: {
        get: async () => {
          throw new Error("down");
        },
      },
      RATE_LIMIT_HASH_KEY: "k".repeat(48),
    } as unknown as Env;
    expect((await rateLimit(env, "203.0.113.11", "link-create")).allowed).toBe(false);
    expect((await rateLimit(env, "203.0.113.11", "link-fetch")).allowed).toBe(true);
  });

  it("never puts a raw IP in a link bucket key, in either store", async () => {
    const keys: string[] = [];
    const db = migratedD1();
    const env = {
      DB: db.d1,
      RATE_LIMIT: {
        get: async () => null,
        put: async (k: string) => void keys.push(k),
      },
      RATE_LIMIT_HASH_KEY: "k".repeat(48),
    } as unknown as Env;
    await rateLimit(env, "203.0.113.12", "link-fetch");
    await rateLimit(env, "203.0.113.12", "link-create");

    // link-fetch is the KV path.
    for (const key of keys) expect(key).not.toContain("203.0.113.12");
    expect(keys[0]).toMatch(/^rl:link-fetch:\d+:[0-9a-f]{32}$/);

    // link-create is the atomic D1 path and must be equally opaque.
    const counters = db.raw
      .prepare("SELECT bucket_key FROM rate_counters")
      .all() as Array<{ bucket_key: string }>;
    expect(counters).toHaveLength(1);
    expect(counters[0]!.bucket_key).not.toContain("203.0.113.12");
    expect(counters[0]!.bucket_key).toMatch(/^rl:link-create:\d+:[0-9a-f]{32}$/);
  });
});
