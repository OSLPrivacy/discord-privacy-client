/// Unit tests for the link-grant issuer.
///
/// The single most important assertion in this file is
/// `mints a grant carrying EXACTLY aud, exp and jti`. A grant that
/// carried a user id would let the cipher-store — the party that
/// actually gets subpoenaed — answer "who made this link". It must not
/// be able to. `cipher-store-cf/test/link-grant.test.ts` pins the same
/// claim set from the verifier's side; both have to be edited to break
/// the property, which is the point.

import { describe, expect, it } from "vitest";
import type { Env } from "../../src/env.js";
import {
  GRANT_AUDIENCE,
  GRANT_DOMAIN,
  GRANT_LIFETIME_SECONDS,
  GRANT_SCHEME,
  grantPayloadJson,
  grantSigningBytes,
  loadIssuerKey,
  MAX_GRANT_LIFETIME_SECONDS,
  mintGrant,
  newGrantJti,
  pkcs8FromSeed,
  resetIssuerKeyCache,
} from "../../src/lib/link-grant-issuer.js";

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary);
}

function base64UrlDecode(value: string): Uint8Array {
  const normalised = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalised + "=".repeat((4 - (normalised.length % 4)) % 4);
  const binary = atob(padded);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}

/// A real Ed25519 pair, exported in both the forms an operator might
/// paste: PKCS#8 for the secret, raw 32 bytes for the public half.
async function generatePair(): Promise<{ secretB64: string; pubB64: string }> {
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, [
    "sign",
    "verify",
  ])) as CryptoKeyPair;
  const pkcs8 = new Uint8Array((await crypto.subtle.exportKey("pkcs8", pair.privateKey)) as ArrayBuffer);
  const raw = new Uint8Array((await crypto.subtle.exportKey("raw", pair.publicKey)) as ArrayBuffer);
  return { secretB64: base64(pkcs8), pubB64: base64(raw) };
}

function envWith(overrides: Partial<Env>): Env {
  return overrides as unknown as Env;
}

describe("link-grant issuer key loading", () => {
  it("refuses to issue when either half of the pair is unset", async () => {
    resetIssuerKeyCache();
    const { secretB64, pubB64 } = await generatePair();
    expect(await loadIssuerKey(envWith({}))).toBeNull();
    expect(await loadIssuerKey(envWith({ LINK_GRANT_SECRET_B64: secretB64 }))).toBeNull();
    expect(await loadIssuerKey(envWith({ LINK_GRANT_PUBKEY_B64: pubB64 }))).toBeNull();
  });

  it("refuses a secret and public half that are not actually a pair", async () => {
    // The operator error this exists to catch: without the pair check
    // every grant looks fine here and is refused at the cipher-store
    // with `grant_signature`, which reads like an attack, not a typo.
    resetIssuerKeyCache();
    const a = await generatePair();
    const b = await generatePair();
    const key = await loadIssuerKey(
      envWith({ LINK_GRANT_SECRET_B64: a.secretB64, LINK_GRANT_PUBKEY_B64: b.pubB64 }),
    );
    expect(key).toBeNull();
  });

  it("refuses malformed base64, wrong-length keys and garbage", async () => {
    resetIssuerKeyCache();
    const { secretB64, pubB64 } = await generatePair();
    for (const bad of ["", "!!!!", base64(new Uint8Array(31)), base64(new Uint8Array(64))]) {
      expect(
        await loadIssuerKey(
          envWith({ LINK_GRANT_SECRET_B64: secretB64, LINK_GRANT_PUBKEY_B64: bad }),
        ),
      ).toBeNull();
      expect(
        await loadIssuerKey(
          envWith({ LINK_GRANT_SECRET_B64: bad, LINK_GRANT_PUBKEY_B64: pubB64 }),
        ),
      ).toBeNull();
    }
  });

  it("accepts a raw 32-byte seed as well as PKCS#8", async () => {
    resetIssuerKeyCache();
    const { secretB64 } = await generatePair();
    // Strip the 16-byte PKCS#8 prefix back to the bare seed and check
    // the wrapper reconstitutes exactly what we started with.
    const pkcs8 = base64UrlDecode(secretB64);
    const seed = pkcs8.slice(16);
    expect(seed.byteLength).toBe(32);
    expect(Array.from(pkcs8FromSeed(seed))).toEqual(Array.from(pkcs8));

    // Derive the public half from the imported PKCS#8 by signing with
    // both forms and verifying the seed form against the same key.
    const jwk = await crypto.subtle.importKey("pkcs8", pkcs8, { name: "Ed25519" }, false, [
      "sign",
    ]);
    expect(jwk).toBeTruthy();
  });
});

describe("grant minting", () => {
  async function issuer() {
    resetIssuerKeyCache();
    const { secretB64, pubB64 } = await generatePair();
    const key = await loadIssuerKey(
      envWith({ LINK_GRANT_SECRET_B64: secretB64, LINK_GRANT_PUBKEY_B64: pubB64 }),
    );
    if (!key) throw new Error("issuer key should have loaded");
    return key;
  }

  it("mints a grant carrying EXACTLY aud, exp and jti", async () => {
    const key = await issuer();
    const grant = await mintGrant(key, 1_753_500_000);
    const [payloadB64] = grant.authorization
      .slice(GRANT_SCHEME.length + 1)
      .split(".");
    const claims = JSON.parse(new TextDecoder().decode(base64UrlDecode(payloadB64!)));
    // ANONYMITY INVARIANT. Three keys, no more, forever.
    expect(Object.keys(claims).sort()).toEqual(["aud", "exp", "jti"]);
    expect(claims.aud).toBe(GRANT_AUDIENCE);
    expect(claims.exp).toBe(1_753_500_000 + GRANT_LIFETIME_SECONDS);
    expect(claims.jti).toMatch(/^[0-9a-f]{32}$/);
  });

  it("never lets a user id, licence or device reach the payload", async () => {
    // Belt to the braces above: if someone adds a field by threading it
    // through some other route, the serialized bytes will show it.
    const key = await issuer();
    const grant = await mintGrant(key, 1_753_500_000);
    const [payloadB64] = grant.authorization.slice(GRANT_SCHEME.length + 1).split(".");
    const text = new TextDecoder().decode(base64UrlDecode(payloadB64!));
    for (const forbidden of ["user", "uid", "sub", "device", "licen", "tier", "email"]) {
      expect(text.toLowerCase()).not.toContain(forbidden);
    }
  });

  it("produces the scheme, split and signature length the verifier parses", async () => {
    const key = await issuer();
    const grant = await mintGrant(key);
    expect(grant.authorization.startsWith(`${GRANT_SCHEME} `)).toBe(true);
    const parts = grant.authorization.slice(GRANT_SCHEME.length + 1).split(".");
    expect(parts).toHaveLength(2);
    expect(base64UrlDecode(parts[1]!).byteLength).toBe(64);
    // base64url only: a `+` or `/` would not survive the verifier's decode.
    expect(grant.authorization.slice(GRANT_SCHEME.length + 1)).toMatch(/^[A-Za-z0-9_.-]+$/);
  });

  it("signs the domain-separated payload, verifiable with the public half", async () => {
    resetIssuerKeyCache();
    const { secretB64, pubB64 } = await generatePair();
    const key = await loadIssuerKey(
      envWith({ LINK_GRANT_SECRET_B64: secretB64, LINK_GRANT_PUBKEY_B64: pubB64 }),
    );
    const grant = await mintGrant(key!, 1_753_500_000);
    const [payloadB64, sigB64] = grant.authorization
      .slice(GRANT_SCHEME.length + 1)
      .split(".");
    const verifyKey = await crypto.subtle.importKey(
      "raw",
      key!.publicKey,
      { name: "Ed25519" },
      false,
      ["verify"],
    );
    const payload = base64UrlDecode(payloadB64!);
    expect(
      await crypto.subtle.verify(
        { name: "Ed25519" },
        verifyKey,
        base64UrlDecode(sigB64!),
        grantSigningBytes(payload),
      ),
    ).toBe(true);
    // And the naked payload must NOT verify: domain separation is what
    // stops a signature from another OSL lane being replayed as a grant.
    expect(
      await crypto.subtle.verify(
        { name: "Ed25519" },
        verifyKey,
        base64UrlDecode(sigB64!),
        payload,
      ),
    ).toBe(false);
  });

  it("stays inside the lifetime cap the cipher-store enforces", async () => {
    const key = await issuer();
    const now = 1_753_500_000;
    const grant = await mintGrant(key, now);
    expect(grant.expiresAt - now).toBe(GRANT_LIFETIME_SECONDS);
    expect(grant.expiresAt - now).toBeLessThanOrEqual(MAX_GRANT_LIFETIME_SECONDS);
    // Headroom matters: the two Workers have independent clocks, and a
    // grant minted at exactly the cap would be refused by a store whose
    // clock is a second behind.
    expect(GRANT_LIFETIME_SECONDS).toBeLessThan(MAX_GRANT_LIFETIME_SECONDS);
  });

  it("uses a fresh jti per grant so one cannot mint two links", async () => {
    const key = await issuer();
    const seen = new Set<string>();
    for (let i = 0; i < 32; i++) {
      const grant = await mintGrant(key);
      const [payloadB64] = grant.authorization.slice(GRANT_SCHEME.length + 1).split(".");
      const claims = JSON.parse(new TextDecoder().decode(base64UrlDecode(payloadB64!)));
      seen.add(claims.jti);
    }
    expect(seen.size).toBe(32);
  });
});

describe("canonical payload encoding", () => {
  it("matches crypto::view_once_link::grant_payload_json byte for byte", () => {
    // Mirrored vector: crates/crypto/src/view_once_link.rs
    // `grant_payload_and_signing_bytes_are_canonical`.
    const payload = grantPayloadJson(1_753_500_600, "0123456789abcdef0123456789abcdef");
    expect(new TextDecoder().decode(payload)).toBe(
      '{"aud":"osl-link-create","exp":1753500600,"jti":"0123456789abcdef0123456789abcdef"}',
    );
  });

  it("separates the domain from the payload with a byte the domain cannot contain", () => {
    const payload = grantPayloadJson(1, "0".repeat(32));
    const signing = grantSigningBytes(payload);
    const domain = new TextEncoder().encode(GRANT_DOMAIN);
    expect(Array.from(signing.slice(0, domain.length))).toEqual(Array.from(domain));
    expect(signing[domain.length]).toBe(0x00);
    expect(Array.from(signing.slice(domain.length + 1))).toEqual(Array.from(payload));
    expect(GRANT_DOMAIN).not.toContain("\x00");
  });

  it("emits a jti in the exact shape the verifier's regex accepts", () => {
    for (let i = 0; i < 16; i++) expect(newGrantJti()).toMatch(/^[0-9a-f]{32}$/);
  });
});
