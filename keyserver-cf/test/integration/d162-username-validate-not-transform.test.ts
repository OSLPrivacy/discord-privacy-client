// D-162. The username endpoints VALIDATE, they never TRANSFORM.
//
// `handleUsernameClaim` / `handleUsernameLookup` refuse any spelling that is
// not already canonical instead of quietly lowercasing or trimming it. That is
// load-bearing: a server-side transform would let one caller claim, and another
// caller resolve, an identifier the user never typed — and the Rust client
// (crates/keystore/src/client.rs:147 `is_normalized_username`) mirrors the same
// grammar and refuses locally with the identical message, so the two sides must
// not drift.
//
// This file exists because commit db4172f7b tried to move canonicalisation into
// the Worker, imported a `normalizeUsername()` that was never written, and left
// the Worker unbundlable for two days. The revert restored validate-don't-
// transform; this test pins that it is really ENFORCED and not merely absent.
//
// Its own mutant: delete the `validNormalizedUsername` guard in
// src/endpoints/usernames.ts and every case below must go 400 -> 200/404.
import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { usernameClaimMessage } from "../../src/lib/username.js";
import { base64Encode, registerTestUser, signEd25519 } from "./helpers.js";

let sequence = 0;
const userId = () => `d162-user-${Date.now()}-${sequence++}`;
const nextIp = () => `198.51.100.${(sequence++ % 200) + 20}`;

function b64url(bytes: Uint8Array): string {
  let value = "";
  for (const byte of bytes) value += String.fromCharCode(byte);
  return btoa(value).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function friendCode(
  user_id: string,
  pair: { publicKeyB64: string; signingKey: CryptoKey },
): Promise<string> {
  const payload = {
    version: 1,
    osl_user_id: user_id,
    x25519_public: base64Encode(new Uint8Array(32).fill(0x11)),
    ed25519_public: pair.publicKeyB64,
    mlkem768_public: base64Encode(new Uint8Array(1184).fill(0x22)),
    ratchet_initial_public: base64Encode(new Uint8Array(32).fill(0x33)),
  };
  const signature = await crypto.subtle.sign(
    { name: "Ed25519" },
    pair.signingKey,
    new TextEncoder().encode(JSON.stringify(payload)),
  );
  const signed = JSON.stringify({ payload, signature: b64url(new Uint8Array(signature)) });
  return `OSLFR1.${b64url(new TextEncoder().encode(signed))}`;
}

async function lookup(username: string): Promise<Response> {
  return SELF.fetch("http://test/v1/usernames/lookup", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": nextIp() },
    body: JSON.stringify({ username }),
  });
}

async function claim(
  username: string,
  uid: string,
  pair: { publicKeyB64: string; signingKey: CryptoKey },
): Promise<Response> {
  const friend_code = await friendCode(uid, pair);
  const request_id = b64url(crypto.getRandomValues(new Uint8Array(32)));
  const timestamp_ms = Date.now();
  const signature_b64 = await signEd25519(
    pair.signingKey,
    usernameClaimMessage({ username, user_id: uid, friend_code, request_id, timestamp_ms }),
  );
  return SELF.fetch("http://test/v1/usernames/claim", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": nextIp() },
    body: JSON.stringify({
      username, user_id: uid, friend_code, request_id, timestamp_ms, signature_b64,
    }),
  });
}

// Every one of these is a spelling a naive "be helpful, just lowercase it"
// server would have accepted and folded onto `alice_01` or `alice`.
const NON_CANONICAL = [
  "Alice",       // leading capital
  "ALICE",       // all caps
  "Alice_01",    // capital, otherwise identical to a real handle
  "al ice",      // interior space
  "alice-01",    // hyphen is not in the grammar
  " alice_01",   // leading whitespace a trim would have eaten
  "alice_01 ",   // trailing whitespace
  "_alice",      // underscore is interior-only
  "alice_",      // underscore is interior-only
  "Ab",          // too short AND non-canonical
];

describe("D-162 username endpoints validate and never transform", () => {
  it("refuses every non-canonical spelling on claim with the shipping message", async () => {
    for (const username of NON_CANONICAL) {
      const uid = userId();
      const pair = await registerTestUser(SELF, uid);
      const response = await claim(username, uid, pair);
      expect(response.status, `claim accepted ${JSON.stringify(username)}`).toBe(400);
      expect(await response.text()).toContain("username must already be normalized");
    }
  });

  it("refuses every non-canonical spelling on lookup with the shipping message", async () => {
    for (const username of NON_CANONICAL) {
      const response = await lookup(username);
      expect(response.status, `lookup accepted ${JSON.stringify(username)}`).toBe(400);
      expect(await response.text()).toContain("username must already be normalized");
    }
  });

  it("stores the byte-exact canonical name and does not resolve a folded spelling", async () => {
    const uid = userId();
    const pair = await registerTestUser(SELF, uid);
    expect((await claim("d162_alice", uid, pair)).status).toBe(200);

    const hit = await lookup("d162_alice");
    expect(hit.status).toBe(200);
    // The row is returned byte-for-byte as claimed: no display/identity split.
    expect(await hit.json()).toMatchObject({ found: true, username: "d162_alice" });

    // A confusable-adjacent but grammatically legal handle is a DIFFERENT
    // identity here. Folding it would be the T5-K2 behaviour, which is parked
    // as D-162 and deliberately absent.
    const distinct = await lookup("d162_a1ice");
    expect(distinct.status).toBe(200);
    expect(await distinct.json()).toMatchObject({ found: false });

    // And the capitalised spelling is refused outright rather than folded onto
    // the row that exists.
    expect((await lookup("D162_alice")).status).toBe(400);
  });
});
