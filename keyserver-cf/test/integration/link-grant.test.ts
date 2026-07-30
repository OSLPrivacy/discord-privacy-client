/// POST /v1/link-grant — the issuer for the non-OSL view-once link lane.
///
/// The route is deliberately UNCONFIGURED in `vitest.config.ts` (no
/// `LINK_GRANT_SECRET_B64` binding), which mirrors production: the key
/// is not installed yet, so the live Worker refuses. The happy paths
/// call the handler directly with an env that has the key, the same
/// pattern `donations.test.ts` uses for Stripe.

import { env, SELF } from "cloudflare:test";
import { beforeAll, describe, expect, it } from "vitest";
import worker from "../../src/index.js";
import type { Env } from "../../src/env.js";
import { handleLinkGrant, LINK_GRANT_DAILY_MAX } from "../../src/endpoints/link-grant.js";
import { canonicalLinkGrantBytes } from "../../src/lib/canonical.js";
import { GRANT_SCHEME, resetIssuerKeyCache } from "../../src/lib/link-grant-issuer.js";
import { base64Encode, generateEd25519Pair, signEd25519, signedRegisterBody } from "./helpers.js";

let n = 0;
const uid = () => `lg-${Date.now()}-${n++}`;

let issuerSecretB64 = "";
let issuerPubB64 = "";

beforeAll(async () => {
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, [
    "sign",
    "verify",
  ])) as CryptoKeyPair;
  issuerSecretB64 = base64Encode(
    new Uint8Array((await crypto.subtle.exportKey("pkcs8", pair.privateKey)) as ArrayBuffer),
  );
  issuerPubB64 = base64Encode(
    new Uint8Array((await crypto.subtle.exportKey("raw", pair.publicKey)) as ArrayBuffer),
  );
});

function configuredEnv(overrides: Partial<Env> = {}): Env {
  resetIssuerKeyCache();
  return {
    ...env,
    LINK_GRANT_SECRET_B64: issuerSecretB64,
    LINK_GRANT_PUBKEY_B64: issuerPubB64,
    ...overrides,
  } as Env;
}

function requestId(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

/// Register a fresh identity and return everything needed to sign for it.
async function registeredIdentity(): Promise<{
  userId: string;
  signingKey: CryptoKey;
}> {
  const pair = await generateEd25519Pair();
  const userId = uid();
  const res = await SELF.fetch("http://test/v1/register", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(await signedRegisterBody(userId, pair)),
  });
  expect(res.status).toBe(201);
  return { userId, signingKey: pair.signingKey };
}

async function signedBody(
  userId: string,
  signingKey: CryptoKey,
  overrides: Record<string, unknown> = {},
): Promise<Record<string, unknown>> {
  const timestampMs = (overrides.timestamp_ms as number) ?? Date.now();
  const rid = (overrides.request_id as string) ?? requestId();
  const message = canonicalLinkGrantBytes({
    user_id: userId,
    timestamp_ms: timestampMs,
    request_id: rid,
  });
  return {
    user_id: userId,
    timestamp_ms: timestampMs,
    request_id: rid,
    signature_b64: await signEd25519(signingKey, message),
    ...overrides,
  };
}

function post(body: unknown): Request {
  return new Request("http://test/v1/link-grant", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

describe("POST /v1/link-grant — deployment gate", () => {
  it("503s on the live route because no issuer key is installed", async () => {
    // This is production's current state, and it is the correct one:
    // with no key, no grant, so the cipher-store creates no links.
    const res = await SELF.fetch("http://test/v1/link-grant", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(503);
    expect(await res.json()).toEqual({ error: "link_grant_not_enabled" });
  });

  it("reaches issuance through the Worker route only when explicitly enabled", async () => {
    const identity = await registeredIdentity();
    const res = await worker.fetch(
      post(await signedBody(identity.userId, identity.signingKey)),
      configuredEnv({ LINK_GRANT_ENABLED: "true" }),
      {} as ExecutionContext,
    );
    expect(res.status).toBe(200);
    await expect(res.json()).resolves.toMatchObject({
      authorization: expect.stringMatching(/^OSL-Link-Grant /),
    });
  });

  it("503s when only one half of the pair is configured", async () => {
    const identity = await registeredIdentity();
    const body = await signedBody(identity.userId, identity.signingKey);
    for (const half of [
      { LINK_GRANT_SECRET_B64: undefined },
      { LINK_GRANT_PUBKEY_B64: undefined },
    ]) {
      const res = await handleLinkGrant(post(body), configuredEnv(half as Partial<Env>));
      expect(res.status).toBe(503);
    }
  });

  it("503s when the configured pair does not match", async () => {
    const other = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, [
      "sign",
      "verify",
    ])) as CryptoKeyPair;
    const strangerPub = base64Encode(
      new Uint8Array((await crypto.subtle.exportKey("raw", other.publicKey)) as ArrayBuffer),
    );
    const identity = await registeredIdentity();
    const body = await signedBody(identity.userId, identity.signingKey);
    const res = await handleLinkGrant(
      post(body),
      configuredEnv({ LINK_GRANT_PUBKEY_B64: strangerPub }),
    );
    expect(res.status).toBe(503);
  });
});

describe("POST /v1/link-grant — issuance", () => {
  it("issues an anonymous grant to a registered, signing identity", async () => {
    const identity = await registeredIdentity();
    const res = await handleLinkGrant(
      post(await signedBody(identity.userId, identity.signingKey)),
      configuredEnv(),
    );
    expect(res.status).toBe(200);
    const body = (await res.json()) as { authorization: string; expires_at: number };
    expect(body.authorization.startsWith(`${GRANT_SCHEME} `)).toBe(true);
    expect(body.expires_at).toBeGreaterThan(Math.floor(Date.now() / 1000));
  });

  it("returns nothing but the grant and its expiry", async () => {
    // The response is a capability, not a record. An echoed user id
    // here would be a small leak on its own and a large one the moment
    // somebody logs the response.
    const identity = await registeredIdentity();
    const res = await handleLinkGrant(
      post(await signedBody(identity.userId, identity.signingKey)),
      configuredEnv(),
    );
    const body = (await res.json()) as Record<string, unknown>;
    expect(Object.keys(body).sort()).toEqual(["authorization", "expires_at"]);
    expect(JSON.stringify(body)).not.toContain(identity.userId);
  });

  it("the issued grant carries no identifier of the requester", async () => {
    const identity = await registeredIdentity();
    const res = await handleLinkGrant(
      post(await signedBody(identity.userId, identity.signingKey)),
      configuredEnv(),
    );
    const body = (await res.json()) as { authorization: string };
    const payloadB64 = body.authorization.slice(GRANT_SCHEME.length + 1).split(".")[0]!;
    const normalised = payloadB64.replace(/-/g, "+").replace(/_/g, "/");
    const padded = normalised + "=".repeat((4 - (normalised.length % 4)) % 4);
    const claims = JSON.parse(atob(padded));
    expect(Object.keys(claims).sort()).toEqual(["aud", "exp", "jti"]);
    expect(JSON.stringify(claims)).not.toContain(identity.userId);
  });

  it("stores no link, URL or token anywhere in the keyserver", async () => {
    // The keyserver must never learn that a link exists. All it may
    // hold is "an identity asked to be vouched for": a request receipt
    // and a day counter.
    const identity = await registeredIdentity();
    await handleLinkGrant(
      post(await signedBody(identity.userId, identity.signingKey)),
      configuredEnv(),
    );
    const receipts = await env.DB.prepare(
      "SELECT user_id, request_digest, expires_at FROM link_grant_receipts WHERE user_id = ?",
    )
      .bind(identity.userId)
      .all();
    expect(receipts.results).toHaveLength(1);
    // Column set is the whole story: three columns, none of which can
    // hold a link id, a capability or a ciphertext.
    expect(Object.keys(receipts.results[0] as object).sort()).toEqual([
      "expires_at",
      "request_digest",
      "user_id",
    ]);
    const quota = await env.DB.prepare(
      "SELECT issued FROM link_grant_quota WHERE user_id = ?",
    )
      .bind(identity.userId)
      .first<{ issued: number }>();
    expect(quota?.issued).toBe(1);
  });
});

describe("POST /v1/link-grant — fail closed", () => {
  it("401s an unregistered identity", async () => {
    const pair = await generateEd25519Pair();
    const res = await handleLinkGrant(
      post(await signedBody(uid(), pair.signingKey)),
      configuredEnv(),
    );
    expect(res.status).toBe(401);
  });

  it("gives the same refusal for an unknown user and a bad signature", async () => {
    // No registration oracle: an attacker must not be able to probe
    // which OSL user ids exist by watching this route's error text.
    const identity = await registeredIdentity();
    const attacker = await generateEd25519Pair();
    const forged = await signedBody(identity.userId, attacker.signingKey);
    const unknown = await signedBody(uid(), attacker.signingKey);
    const a = await handleLinkGrant(post(forged), configuredEnv());
    const b = await handleLinkGrant(post(unknown), configuredEnv());
    expect(a.status).toBe(401);
    expect(b.status).toBe(401);
    expect(await a.text()).toBe(await b.text());
  });

  it("401s a signature made by the wrong key", async () => {
    const identity = await registeredIdentity();
    const attacker = await generateEd25519Pair();
    const res = await handleLinkGrant(
      post(await signedBody(identity.userId, attacker.signingKey)),
      configuredEnv(),
    );
    expect(res.status).toBe(401);
  });

  it("401s a stale or future timestamp even with a valid signature", async () => {
    const identity = await registeredIdentity();
    for (const skew of [-10 * 60 * 1000, 10 * 60 * 1000]) {
      const body = await signedBody(identity.userId, identity.signingKey, {
        timestamp_ms: Date.now() + skew,
      });
      const res = await handleLinkGrant(post(body), configuredEnv());
      expect(res.status).toBe(401);
    }
  });

  it("401s when the signed timestamp is altered in transit", async () => {
    const identity = await registeredIdentity();
    const body = await signedBody(identity.userId, identity.signingKey);
    body.timestamp_ms = (body.timestamp_ms as number) - 1;
    const res = await handleLinkGrant(post(body), configuredEnv());
    expect(res.status).toBe(401);
  });

  it("409s a replayed request_id — one signed request, one grant", async () => {
    // An on-path capture of an issuance request must not be worth
    // replaying: TLS already stops it, this makes it worthless anyway.
    const identity = await registeredIdentity();
    const body = await signedBody(identity.userId, identity.signingKey);
    expect((await handleLinkGrant(post(body), configuredEnv())).status).toBe(200);
    expect((await handleLinkGrant(post(body), configuredEnv())).status).toBe(409);
  });

  it("400s malformed bodies and missing fields", async () => {
    const identity = await registeredIdentity();
    const valid = await signedBody(identity.userId, identity.signingKey);
    const bad: Record<string, unknown>[] = [
      { ...valid, user_id: "" },
      { ...valid, user_id: "has\nnewline" },
      { ...valid, request_id: "too-short" },
      { ...valid, signature_b64: "" },
    ];
    for (const body of bad) {
      const res = await handleLinkGrant(post(body), configuredEnv());
      expect(res.status).toBe(400);
    }
    const malformed = new Request("http://test/v1/link-grant", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: "{ not json",
    });
    expect((await handleLinkGrant(malformed, configuredEnv())).status).toBe(400);
  });

  it("caps one identity at the daily quota and refuses past it", async () => {
    const identity = await registeredIdentity();
    // Seed the counter rather than issuing 50 grants: the assertion is
    // about the trigger, not about doing the work 50 times.
    const day = Math.floor(Date.now() / 1000 / 86_400);
    await env.DB.prepare(
      "INSERT INTO link_grant_quota (user_id, day, issued) VALUES (?, ?, ?)",
    )
      .bind(identity.userId, day, LINK_GRANT_DAILY_MAX)
      .run();
    const res = await handleLinkGrant(
      post(await signedBody(identity.userId, identity.signingKey)),
      configuredEnv(),
    );
    expect(res.status).toBe(429);
    expect(Number(res.headers.get("retry-after"))).toBeGreaterThan(0);
  });

  it("does not burn the request_id when the quota refuses", async () => {
    // A refused request must be retryable tomorrow with the same signed
    // body; burning the id on refusal would make the two failure modes
    // compound into a permanently dead request.
    const identity = await registeredIdentity();
    const day = Math.floor(Date.now() / 1000 / 86_400);
    await env.DB.prepare(
      "INSERT INTO link_grant_quota (user_id, day, issued) VALUES (?, ?, ?)",
    )
      .bind(identity.userId, day, LINK_GRANT_DAILY_MAX)
      .run();
    const body = await signedBody(identity.userId, identity.signingKey);
    expect((await handleLinkGrant(post(body), configuredEnv())).status).toBe(429);
    const receipts = await env.DB.prepare(
      "SELECT COUNT(*) AS c FROM link_grant_receipts WHERE user_id = ?",
    )
      .bind(identity.userId)
      .first<{ c: number }>();
    expect(receipts?.c).toBe(0);
  });

  it("does not spend quota on a replayed request", async () => {
    const identity = await registeredIdentity();
    const body = await signedBody(identity.userId, identity.signingKey);
    await handleLinkGrant(post(body), configuredEnv());
    await handleLinkGrant(post(body), configuredEnv());
    const quota = await env.DB.prepare(
      "SELECT issued FROM link_grant_quota WHERE user_id = ?",
    )
      .bind(identity.userId)
      .first<{ issued: number }>();
    expect(quota?.issued).toBe(1);
  });

  it("counts every issued grant toward the identity's day bucket", async () => {
    const identity = await registeredIdentity();
    for (let i = 0; i < 3; i++) {
      const res = await handleLinkGrant(
        post(await signedBody(identity.userId, identity.signingKey)),
        configuredEnv(),
      );
      expect(res.status).toBe(200);
    }
    const quota = await env.DB.prepare(
      "SELECT issued FROM link_grant_quota WHERE user_id = ?",
    )
      .bind(identity.userId)
      .first<{ issued: number }>();
    expect(quota?.issued).toBe(3);
  });

  it("rejects non-POST methods on the route", async () => {
    for (const method of ["GET", "DELETE", "PUT"]) {
      const res = await SELF.fetch("http://test/v1/link-grant", { method });
      expect([404, 405]).toContain(res.status);
    }
  });
});
