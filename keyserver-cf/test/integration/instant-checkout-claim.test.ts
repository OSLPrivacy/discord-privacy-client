import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  completeStripeCheckoutClaim,
  insertStripeCheckoutClaim,
  prepareStripeCheckoutClaim,
  sweepStripeCheckoutClaims,
} from "../../src/lib/stripe-checkout-claims.js";

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function token(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return base64(bytes).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

describe("browser bound instant Stripe activation", () => {
  it("deletes completed delivery ciphertext after the recovery deadline", async () => {
    const sessionId = `cs_live_expired_${crypto.randomUUID().replace(/-/g, "")}`;
    await env.DB.prepare(
      `INSERT INTO stripe_checkout_claims (
         session_id, claim_hash, delivery_public_key_spki,
         encrypted_license, license_hash, subscription_id, status,
         created_at, expires_at, delivered_at
       ) VALUES (?, ?, 'public-key', 'ciphertext', ?, ?, 'delivery_ready', ?, ?, NULL)`,
    ).bind(
      sessionId,
      `claim-${sessionId}`,
      `license-${sessionId}`,
      `sub-${sessionId}`,
      Math.floor(Date.now() / 1000) - 100,
      Math.floor(Date.now() / 1000) - 1,
    ).run();
    expect(await sweepStripeCheckoutClaims(env.DB)).toBeGreaterThanOrEqual(1);
    const row = await env.DB.prepare(
      "SELECT 1 AS present FROM stripe_checkout_claims WHERE session_id = ?",
    ).bind(sessionId).first<{ present: number }>();
    expect(row).toBeNull();
  });

  it("re-fetches one ciphertext until authenticated ACK, then tombstones it", async () => {
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
    const publicSpki = base64(new Uint8Array(
      await crypto.subtle.exportKey("spki", pair.publicKey) as ArrayBuffer,
    ));
    const claimToken = token();
    const sessionId = `cs_live_claim_${crypto.randomUUID().replace(/-/g, "")}`;
    const subscriptionId = `sub_claim_${crypto.randomUUID().replace(/-/g, "")}`;
    const prepared = await prepareStripeCheckoutClaim({
      claimToken,
      deliveryPublicKeySpki: publicSpki,
      licenseHmacSecret: "test-instant-checkout-secret",
    });
    await insertStripeCheckoutClaim(env.DB, {
      sessionId,
      ...prepared,
      deliveryPublicKeySpki: publicSpki,
      expiresAt: Math.floor(Date.now() / 1000) + 3600,
    });

    const pending = await SELF.fetch("http://test/v1/checkout/claim", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ session_id: sessionId, claim_token: claimToken }),
    });
    expect(pending.status).toBe(200);
    await expect(pending.json()).resolves.toMatchObject({ status: "pending" });

    const wrong = await SELF.fetch("http://test/v1/checkout/claim", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ session_id: sessionId, claim_token: token() }),
    });
    expect(wrong.status).toBe(403);

    expect(await completeStripeCheckoutClaim(env.DB, {
      sessionId,
      subscriptionId,
      customerId: "cus_instant_claim",
      customerEmail: "receipt@example.test",
    })).toBe("completed");
    expect(await completeStripeCheckoutClaim(env.DB, {
      sessionId,
      subscriptionId,
      customerId: "cus_instant_claim",
      customerEmail: "receipt@example.test",
    })).toBe("already_completed");

    const ready = await SELF.fetch("http://test/v1/checkout/claim", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ session_id: sessionId, claim_token: claimToken }),
    });
    expect(ready.status).toBe(200);
    const result = await ready.json() as {
      status: string;
      encrypted_license: string;
      delivery: string;
    };
    expect(result).toMatchObject({
      status: "delivery_ready",
      delivery: "rsa-oaep-sha256",
    });
    const ciphertext = Uint8Array.from(
      atob(result.encrypted_license),
      (character) => character.charCodeAt(0),
    );
    const plaintext = await crypto.subtle.decrypt(
      { name: "RSA-OAEP" },
      pair.privateKey,
      ciphertext,
    );
    expect(new TextDecoder().decode(plaintext)).toMatch(
      /^OSL-[0-9A-HJKMNP-TV-Z]{4}(?:-[0-9A-HJKMNP-TV-Z]{4}){3}$/,
    );

    const retry = await SELF.fetch("http://test/v1/checkout/claim", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ session_id: sessionId, claim_token: claimToken }),
    });
    await expect(retry.json()).resolves.toMatchObject({
      status: "delivery_ready",
      encrypted_license: result.encrypted_license,
    });

    const acknowledged = await SELF.fetch("http://test/v1/checkout/claim", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        session_id: sessionId,
        claim_token: claimToken,
        acknowledge_delivery: true,
      }),
    });
    await expect(acknowledged.json()).resolves.toMatchObject({ status: "acknowledged" });
    const repeatedAck = await SELF.fetch("http://test/v1/checkout/claim", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        session_id: sessionId,
        claim_token: claimToken,
        acknowledge_delivery: true,
      }),
    });
    await expect(repeatedAck.json()).resolves.toMatchObject({
      status: "acknowledged",
      already_acknowledged: true,
    });
    const afterAck = await SELF.fetch("http://test/v1/checkout/claim", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ session_id: sessionId, claim_token: claimToken }),
    });
    expect(afterAck.status).toBe(410);
    const tombstone = await env.DB.prepare(
      "SELECT encrypted_license, acknowledged_at FROM stripe_checkout_claims WHERE session_id = ?",
    ).bind(sessionId).first<{ encrypted_license: string; acknowledged_at: number | null }>();
    expect(tombstone?.encrypted_license).toBe("");
    expect(tombstone?.acknowledged_at).not.toBeNull();

    const count = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM licenses WHERE subscription_id = ?",
    ).bind(subscriptionId).first<{ count: number }>();
    expect(count?.count).toBe(1);
  });
});
