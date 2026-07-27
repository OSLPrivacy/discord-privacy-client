import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  GRANT_AUDIENCE,
  GRANT_DOMAIN,
  GRANT_SCHEME,
  MAX_GRANT_LIFETIME_SECONDS,
  verifyLinkGrant,
} from "../src/lib/link-grant.js";
import { sweepExpiredLinkGrantConsumptions } from "../src/lib/sweep.js";
import { d1Count, d1Run, workerEnv } from "./helpers/workerd.js";
import {
  ISSUER_GRANT_AUDIENCE,
  ISSUER_GRANT_DOMAIN,
  ISSUER_GRANT_SCHEME,
  mintKeyserverGrantFixture,
} from "./helpers/link-grant-issuer-fixture.js";

function b64u(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function issuer() {
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, [
    "sign",
    "verify",
  ])) as CryptoKeyPair;
  const raw = new Uint8Array(
    (await crypto.subtle.exportKey("raw", pair.publicKey)) as ArrayBuffer,
  );
  let bin = "";
  for (const b of raw) bin += String.fromCharCode(b);
  return { pair, pubB64: btoa(bin) };
}

function randomJti(): string {
  return [...crypto.getRandomValues(new Uint8Array(16))]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function sign(
  pair: CryptoKeyPair,
  claims: Record<string, unknown>,
  domain = GRANT_DOMAIN,
): Promise<string> {
  const payload = new TextEncoder().encode(JSON.stringify(claims));
  const dom = new TextEncoder().encode(domain);
  const message = new Uint8Array(dom.byteLength + 1 + payload.byteLength);
  message.set(dom, 0);
  message[dom.byteLength] = 0;
  message.set(payload, dom.byteLength + 1);
  const sig = new Uint8Array(
    await crypto.subtle.sign({ name: "Ed25519" }, pair.privateKey, message),
  );
  return `${GRANT_SCHEME} ${b64u(payload)}.${b64u(sig)}`;
}

function testEnv(pubB64?: string): Env {
  return workerEnv(pubB64 ? { LINK_GRANT_PUBKEY_B64: pubB64 } : {});
}

function request(auth?: string): Request {
  return new Request("https://links.test/v1/link", {
    method: "POST",
    headers: auth ? { authorization: auth } : {},
  });
}

const now = () => Math.floor(Date.now() / 1000);

describe("link-creation grants", () => {
  it("accepts the standalone keyserver issuer wire fixture", async () => {
    const iss = await issuer();
    expect({
      audience: ISSUER_GRANT_AUDIENCE,
      domain: ISSUER_GRANT_DOMAIN,
      scheme: ISSUER_GRANT_SCHEME,
    }).toEqual({
      audience: GRANT_AUDIENCE,
      domain: GRANT_DOMAIN,
      scheme: GRANT_SCHEME,
    });
    const issuedAt = now();
    const grant = await mintKeyserverGrantFixture(iss.pair.privateKey, issuedAt);
    const claims = JSON.parse(grant.payload) as Record<string, unknown>;
    expect(Object.keys(claims).sort()).toEqual(["aud", "exp", "jti"]);
    expect(grant.payload).toBe(
      `{"aud":"osl-link-create","exp":${issuedAt + 300},"jti":"${claims.jti}"}`,
    );
    await expect(
      verifyLinkGrant(request(grant.authorization), testEnv(iss.pubB64)),
    ).resolves.toEqual({ ok: true });
  });

  it("accepts a fresh, correctly signed, anonymous grant", async () => {
    const iss = await issuer();
    const env = testEnv(iss.pubB64);
    const auth = await sign(iss.pair, {
      aud: GRANT_AUDIENCE,
      exp: now() + 120,
      jti: randomJti(),
    });
    await expect(verifyLinkGrant(request(auth), env)).resolves.toEqual({ ok: true });
  });

  it("carries no identity: the grant claims are aud, exp and jti only", async () => {
    const iss = await issuer();
    const env = testEnv(iss.pubB64);
    const claims = { aud: GRANT_AUDIENCE, exp: now() + 120, jti: randomJti() };
    // The cipher-store must not learn who created a link. If a future
    // change adds a user id here, this test is where it should hurt.
    expect(Object.keys(claims).sort()).toEqual(["aud", "exp", "jti"]);
    const result = await verifyLinkGrant(request(await sign(iss.pair, claims)), env);
    expect(result).toEqual({ ok: true });
  });

  it("refuses everything when no issuer key is configured", async () => {
    const iss = await issuer();
    const env = testEnv();
    const auth = await sign(iss.pair, {
      aud: GRANT_AUDIENCE,
      exp: now() + 120,
      jti: randomJti(),
    });
    const result = await verifyLinkGrant(request(auth), env);
    expect(result).toMatchObject({ ok: false, status: 503 });
  });

  it("rejects a missing or malformed authorization header", async () => {
    const iss = await issuer();
    const env = testEnv(iss.pubB64);
    for (const auth of [
      undefined,
      "Bearer abc",
      `${GRANT_SCHEME} onepart`,
      `${GRANT_SCHEME} a.b.c`,
      `${GRANT_SCHEME} !!!.???`,
    ]) {
      const result = await verifyLinkGrant(request(auth), env);
      expect(result).toMatchObject({ ok: false, status: 401 });
    }
  });

  it("rejects a grant signed by a different key", async () => {
    const good = await issuer();
    const bad = await issuer();
    const env = testEnv(good.pubB64);
    const auth = await sign(bad.pair, {
      aud: GRANT_AUDIENCE,
      exp: now() + 120,
      jti: randomJti(),
    });
    expect(await verifyLinkGrant(request(auth), env)).toMatchObject({
      ok: false,
      code: "grant_signature",
    });
  });

  it("rejects a grant signed under a different domain separator", async () => {
    const iss = await issuer();
    const env = testEnv(iss.pubB64);
    const auth = await sign(
      iss.pair,
      { aud: GRANT_AUDIENCE, exp: now() + 120, jti: randomJti() },
      "SOME-OTHER-DOMAIN-v1",
    );
    expect(await verifyLinkGrant(request(auth), env)).toMatchObject({
      ok: false,
      code: "grant_signature",
    });
  });

  it("rejects a wrong audience, an expired grant, and an over-long lifetime", async () => {
    const iss = await issuer();
    const env = testEnv(iss.pubB64);
    const wrongAud = await sign(iss.pair, {
      aud: "something-else",
      exp: now() + 120,
      jti: randomJti(),
    });
    expect(await verifyLinkGrant(request(wrongAud), env)).toMatchObject({
      code: "grant_audience",
    });

    const expired = await sign(iss.pair, {
      aud: GRANT_AUDIENCE,
      exp: now() - 1,
      jti: randomJti(),
    });
    expect(await verifyLinkGrant(request(expired), env)).toMatchObject({
      code: "grant_expired",
    });

    const tooLong = await sign(iss.pair, {
      aud: GRANT_AUDIENCE,
      exp: now() + MAX_GRANT_LIFETIME_SECONDS + 60,
      jti: randomJti(),
    });
    expect(await verifyLinkGrant(request(tooLong), env)).toMatchObject({
      code: "grant_lifetime",
    });
  });

  it("is single-use: the same grant cannot mint two links", async () => {
    const iss = await issuer();
    const env = testEnv(iss.pubB64);
    const jti = randomJti();
    const auth = await sign(iss.pair, {
      aud: GRANT_AUDIENCE,
      exp: now() + 120,
      jti,
    });
    expect(await verifyLinkGrant(request(auth), env)).toEqual({ ok: true });
    expect(await verifyLinkGrant(request(auth), env)).toMatchObject({
      code: "grant_replay",
    });
    expect(
      await d1Count("SELECT COUNT(*) AS c FROM link_grant_consumed WHERE jti = ?", jti),
    ).toBe(1);
  });

  it("is atomic under concurrent presentation of the same grant", async () => {
    const iss = await issuer();
    const env = testEnv(iss.pubB64);
    const auth = await sign(iss.pair, {
      aud: GRANT_AUDIENCE,
      exp: now() + 120,
      jti: randomJti(),
    });

    const results = await Promise.all([
      verifyLinkGrant(request(auth), env),
      verifyLinkGrant(request(auth), env),
    ]);
    expect(results.filter((result) => result.ok).length).toBe(1);
    expect(results.filter((result) => !result.ok && result.code === "grant_replay").length).toBe(1);
  });

  it("accepts a different grant after consuming one grant", async () => {
    const iss = await issuer();
    const env = testEnv(iss.pubB64);
    const first = await sign(iss.pair, {
      aud: GRANT_AUDIENCE,
      exp: now() + 120,
      jti: randomJti(),
    });
    const second = await sign(iss.pair, {
      aud: GRANT_AUDIENCE,
      exp: now() + 120,
      jti: randomJti(),
    });

    expect(await verifyLinkGrant(request(first), env)).toEqual({ ok: true });
    expect(await verifyLinkGrant(request(second), env)).toEqual({ ok: true });
  });

  it("fails closed when the replay store is unavailable", async () => {
    const iss = await issuer();
    const env = {
      LINK_GRANT_PUBKEY_B64: iss.pubB64,
      DB: {
        prepare: () => ({
          bind: () => ({
            first: async () => {
              throw new Error("d1 down");
            },
          }),
        }),
      },
    } as unknown as Env;
    const auth = await sign(iss.pair, {
      aud: GRANT_AUDIENCE,
      exp: now() + 120,
      jti: randomJti(),
    });
    expect(await verifyLinkGrant(request(auth), env)).toMatchObject({
      ok: false,
      status: 503,
    });
  });

  it("sweeps expired grant consumption records in bounded batches", async () => {
    const expired = now() - 1;
    const live = now() + 120;
    for (let index = 0; index < 101; index++) {
      await d1Run(
        "INSERT INTO link_grant_consumed (jti, expires_at) VALUES (?, ?)",
        index.toString(16).padStart(32, "0"),
        expired,
      );
    }
    await d1Run(
      "INSERT INTO link_grant_consumed (jti, expires_at) VALUES (?, ?)",
      "f".repeat(32),
      live,
    );

    await expect(sweepExpiredLinkGrantConsumptions(workerEnv())).resolves.toBe(101);
    expect(await d1Count("SELECT COUNT(*) AS c FROM link_grant_consumed WHERE expires_at < ?", now())).toBe(0);
    expect(await d1Count("SELECT COUNT(*) AS c FROM link_grant_consumed")).toBe(1);
  });
});
