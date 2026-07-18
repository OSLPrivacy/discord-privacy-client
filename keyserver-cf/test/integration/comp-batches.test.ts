import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";

const AUTH = {
  authorization: "Bearer test-admin-token-do-not-ship",
  "x-osl-comp-authorization": "Bearer test-comp-admin-token-do-not-ship",
  "content-type": "application/json",
};

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function requestId(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return base64(bytes).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

async function deliveryPair(): Promise<{ pair: CryptoKeyPair; spki: string }> {
  const pair = await crypto.subtle.generateKey(
    {
      name: "RSA-OAEP",
      modulusLength: 2048,
      publicExponent: new Uint8Array([1, 0, 1]),
      hash: "SHA-256",
    },
    true,
    ["encrypt", "decrypt"],
  ) as CryptoKeyPair;
  const exported = await crypto.subtle.exportKey("spki", pair.publicKey) as ArrayBuffer;
  return { pair, spki: base64(new Uint8Array(exported)) };
}

async function decryptDelivery(
  pair: CryptoKeyPair,
  delivery: { wrapped_key: string; nonce: string; ciphertext: string },
): Promise<{ batch_id: string; activation_codes: string[] }> {
  const wrapped = Uint8Array.from(atob(delivery.wrapped_key), (value) => value.charCodeAt(0));
  const raw = await crypto.subtle.decrypt({ name: "RSA-OAEP" }, pair.privateKey, wrapped);
  const aes = await crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["decrypt"]);
  const nonce = Uint8Array.from(atob(delivery.nonce), (value) => value.charCodeAt(0));
  const ciphertext = Uint8Array.from(atob(delivery.ciphertext), (value) => value.charCodeAt(0));
  const plaintext = await crypto.subtle.decrypt(
    {
      name: "AES-GCM",
      iv: nonce,
      additionalData: new TextEncoder().encode("osl-comp-delivery-v1"),
    },
    aes,
    ciphertext,
  );
  return JSON.parse(new TextDecoder().decode(plaintext)) as {
    batch_id: string;
    activation_codes: string[];
  };
}

describe("owner comp batches", () => {
  it("issues at most 25 codes through dual auth and one encrypted response", async () => {
    const { pair, spki } = await deliveryPair();
    const body = {
      quantity: 2,
      purpose: "owner approved launch review",
      expires_at: Math.floor(Date.now() / 1000) + 7 * 86400,
      request_id: requestId(),
      delivery_public_key_spki: spki,
    };
    const response = await SELF.fetch("http://test/v1/internal/comp/batches", {
      method: "POST",
      headers: AUTH,
      body: JSON.stringify(body),
    });
    expect(response.status).toBe(201);
    const result = await response.json() as {
      batch_id: string;
      audit_digest: string;
      delivery: { wrapped_key: string; nonce: string; ciphertext: string };
    };
    expect(result.audit_digest).toMatch(/^[0-9a-f]{64}$/);
    const decrypted = await decryptDelivery(pair, result.delivery);
    expect(decrypted.batch_id).toBe(result.batch_id);
    expect(decrypted.activation_codes).toHaveLength(2);
    expect(decrypted.activation_codes.every((code) =>
      /^OSL-[0-9A-HJKMNP-TV-Z]{4}(?:-[0-9A-HJKMNP-TV-Z]{4}){3}$/.test(code)
    )).toBe(true);

    const stored = await env.DB.prepare(
      `SELECT b.purpose_hash, b.audit_digest, l.license_hash
         FROM comp_batches b JOIN comp_batch_licenses l USING(batch_id)
        WHERE b.batch_id = ?`,
    ).bind(result.batch_id).all<Record<string, string>>();
    const serialized = JSON.stringify(stored.results);
    for (const code of decrypted.activation_codes) expect(serialized).not.toContain(code);

    const replay = await SELF.fetch("http://test/v1/internal/comp/batches", {
      method: "POST",
      headers: AUTH,
      body: JSON.stringify(body),
    });
    expect(replay.status).toBe(409);

    const revoke = await SELF.fetch(
      `http://test/v1/internal/comp/batches/${result.batch_id}`,
      { method: "DELETE", headers: AUTH },
    );
    expect(revoke.status).toBe(200);
    const validation = await SELF.fetch("http://test/v1/license/validate", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ license_key: decrypted.activation_codes[0] }),
    });
    await expect(validation.json()).resolves.toMatchObject({ status: "REVOKED" });
  });

  it("fails closed without both secrets and rejects permanent/oversized batches", async () => {
    const { spki } = await deliveryPair();
    const valid = {
      quantity: 1,
      purpose: "bounded test grant",
      expires_at: Math.floor(Date.now() / 1000) + 86400,
      request_id: requestId(),
      delivery_public_key_spki: spki,
    };
    const missingSecond = await SELF.fetch("http://test/v1/internal/comp/batches", {
      method: "POST",
      headers: { authorization: AUTH.authorization, "content-type": "application/json" },
      body: JSON.stringify(valid),
    });
    expect(missingSecond.status).toBe(401);
    const oversized = await SELF.fetch("http://test/v1/internal/comp/batches", {
      method: "POST",
      headers: AUTH,
      body: JSON.stringify({ ...valid, quantity: 26, request_id: requestId() }),
    });
    expect(oversized.status).toBe(400);
    const permanent = await SELF.fetch("http://test/v1/internal/comp/batches", {
      method: "POST",
      headers: AUTH,
      body: JSON.stringify({
        ...valid,
        expires_at: Math.floor(Date.now() / 1000) + 10 * 365 * 86400,
        request_id: requestId(),
      }),
    });
    expect(permanent.status).toBe(400);
  });
});
