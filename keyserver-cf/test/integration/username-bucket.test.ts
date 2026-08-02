import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { usernameClaimMessage } from "../../src/lib/username.js";
import { base64Encode, generateEd25519Pair, registerTestUser, signEd25519, STUB_MLKEM_PUB_B64, STUB_RATCHET_PUB_B64, STUB_X25519_PUB_B64 } from "./helpers.js";

function url(bytes: Uint8Array): string { return btoa(String.fromCharCode(...bytes)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, ""); }
async function bucket(name: string): Promise<{ prefix: string; suffix: string }> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`OSL-USERNAME-BUCKET-v1${name}`)));
  const value = Array.from(digest, (b) => b.toString(16).padStart(2, "0")).join("");
  return { prefix: value.slice(0, 4), suffix: value.slice(4) };
}
async function claim(name: string, id: string, pair: { publicKeyB64: string; signingKey: CryptoKey }) {
  const payload = { version: 1, osl_user_id: id, x25519_public: STUB_X25519_PUB_B64, ed25519_public: pair.publicKeyB64, mlkem768_public: STUB_MLKEM_PUB_B64, ratchet_initial_public: STUB_RATCHET_PUB_B64 };
  const invite = `OSLFR1.${url(new TextEncoder().encode(JSON.stringify({ payload, signature: url(new Uint8Array(await crypto.subtle.sign({ name: "Ed25519" }, pair.signingKey, new TextEncoder().encode(JSON.stringify(payload))))) })))} `;
  const request_id = url(crypto.getRandomValues(new Uint8Array(32))); const timestamp_ms = Date.now();
  const signature_b64 = await signEd25519(pair.signingKey, usernameClaimMessage({ username: name, user_id: id, friend_code: invite.trim(), request_id, timestamp_ms }));
  return SELF.fetch("http://test/v1/usernames/claim", { method: "POST", headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.90" }, body: JSON.stringify({ username: name, user_id: id, friend_code: invite.trim(), request_id, timestamp_ms, signature_b64 }) });
}

describe("username bucket", () => {
  it("returns sorted, deterministic 1024-row buckets containing the matching key", async () => {
    const name = "bucket_alice"; const id = `bucket-${Date.now()}`; const pair = await registerTestUser(SELF, id);
    expect((await claim(name, id, pair)).status).toBe(200);
    const { prefix, suffix } = await bucket(name);
    const first = await SELF.fetch(`http://test/v1/username-bucket/${prefix}`, { headers: { "cf-connecting-ip": "203.0.113.91" } });
    const second = await SELF.fetch(`http://test/v1/username-bucket/${prefix}`, { headers: { "cf-connecting-ip": "203.0.113.92" } });
    expect(first.status).toBe(200); expect(await first.text()).toBe(await second.text());
    const rows = (await (await SELF.fetch(`http://test/v1/username-bucket/${prefix}`, { headers: { "cf-connecting-ip": "203.0.113.93" } })).text()).trim().split("\n");
    expect(rows).toHaveLength(1024); expect(rows).toEqual([...rows].sort());
    expect(rows).toContain(`${suffix}:${id}:${pair.publicKeyB64}`);
  });
});
