import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  STORAGE_GRANT_AUDIENCE,
  STORAGE_GRANT_DOMAIN,
  STORAGE_GRANT_SCHEME,
  verifyStorageGrant,
} from "../src/lib/storage-grant.js";
import { workerEnv } from "./helpers/workerd.js";

function b64u(bytes: Uint8Array): string {
  let output = "";
  for (const byte of bytes) output += String.fromCharCode(byte);
  return btoa(output).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function issuer() {
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"])) as CryptoKeyPair;
  const raw = new Uint8Array((await crypto.subtle.exportKey("raw", pair.publicKey)) as ArrayBuffer);
  let binary = "";
  for (const byte of raw) binary += String.fromCharCode(byte);
  return { pair, pubB64: btoa(binary) };
}

function jti(): string {
  return [...crypto.getRandomValues(new Uint8Array(16))]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

async function grant(pair: CryptoKeyPair, payload: string): Promise<string> {
  const bytes = new TextEncoder().encode(payload);
  const domain = new TextEncoder().encode(STORAGE_GRANT_DOMAIN);
  const signed = new Uint8Array(domain.byteLength + 1 + bytes.byteLength);
  signed.set(domain, 0);
  signed[domain.byteLength] = 0;
  signed.set(bytes, domain.byteLength + 1);
  const signature = new Uint8Array(await crypto.subtle.sign({ name: "Ed25519" }, pair.privateKey, signed));
  return `${STORAGE_GRANT_SCHEME} ${b64u(bytes)}.${b64u(signature)}`;
}

function request(authorization: string): Request {
  return new Request("https://cipher.test/v1/blob", { method: "POST", headers: { authorization } });
}

describe("storage grants", () => {
  it("accepts a valid anonymous grant and consumes it exactly once", async () => {
    const iss = await issuer();
    const expiresAt = Math.floor(Date.now() / 1000) + 120;
    const nonce = jti();
    // This is the actual signed wire payload, pinned in canonical field order.
    const payload = `{"aud":"osl-blob-store","exp":${expiresAt},"jti":"${nonce}"}`;
    expect(payload).toBe(`{"aud":"${STORAGE_GRANT_AUDIENCE}","exp":${expiresAt},"jti":"${nonce}"}`);
    const authorization = await grant(iss.pair, payload);
    const env = workerEnv({ LINK_GRANT_PUBKEY_B64: iss.pubB64 }) as Env;

    await expect(verifyStorageGrant(request(authorization), env)).resolves.toEqual({ ok: true });
    await expect(verifyStorageGrant(request(authorization), env)).resolves.toMatchObject({
      ok: false,
      status: 401,
      code: "grant_replay",
    });
  });

  it("rejects expired grants and claims with identity or tier fields", async () => {
    const iss = await issuer();
    const env = workerEnv({ LINK_GRANT_PUBKEY_B64: iss.pubB64 }) as Env;
    const expired = await grant(iss.pair, `{"aud":"osl-blob-store","exp":${Math.floor(Date.now() / 1000) - 1},"jti":"${jti()}"}`);
    await expect(verifyStorageGrant(request(expired), env)).resolves.toMatchObject({
      ok: false,
      status: 401,
      code: "grant_expired",
    });

    const withTier = await grant(iss.pair, `{"aud":"osl-blob-store","exp":${Math.floor(Date.now() / 1000) + 120},"jti":"${jti()}","tier":"pro"}`);
    await expect(verifyStorageGrant(request(withTier), env)).resolves.toMatchObject({
      ok: false,
      status: 401,
      code: "grant_claims",
    });
  });
});
