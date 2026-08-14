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
import { base64Encode, publicNameProofFields, registerTestUser, signEd25519 } from "./helpers.js";

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
  const proofFields = await publicNameProofFields(SELF, uid, pair.signingKey, username);
  return SELF.fetch("http://test/v1/usernames/claim", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": nextIp() },
    body: JSON.stringify({
      username, user_id: uid, friend_code, request_id, timestamp_ms, signature_b64, ...proofFields,
    }),
  });
}

// Every one of these is outside the public-name grammar and must be refused
// rather than trimmed or otherwise transformed.
const NON_CANONICAL = [
  "al ice",      // interior space
  "alice-01",    // hyphen is not in the grammar
  " alice_01",   // leading whitespace a trim would have eaten
  "alice_01 ",   // trailing whitespace
  "a".repeat(17), // exceeds the 16-character maximum
];

describe("D-162 username endpoints validate and never transform", () => {
  it("refuses every non-canonical spelling on claim with the shipping message", async () => {
    for (const username of NON_CANONICAL) {
      const uid = userId();
      const pair = await registerTestUser(SELF, uid);
      const response = await claim(username, uid, pair);
      expect(response.status, `claim accepted ${JSON.stringify(username)}`).toBe(400);
      expect(await response.text()).toContain("username must use only letters, digits, and underscores and be 1 to 16 characters");
    }
  });

  it("refuses every non-canonical spelling on lookup with the shipping message", async () => {
    for (const username of NON_CANONICAL) {
      const response = await lookup(username);
      expect(response.status, `lookup accepted ${JSON.stringify(username)}`).toBe(400);
      expect(await response.text()).toContain("username must use only letters, digits, and underscores and be 1 to 16 characters");
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

    // ── CHANGED BY D-248, and the change is a contract decision, not a fix
    // to make a suite pass. ─────────────────────────────────────────────────
    // This block used to assert that a second user MAY claim `d162_a1ice`,
    // with the note that "under T5-K2's skeleton index this claim was a 409".
    // The skeleton index is now live: migration 0038 creates
    // `CREATE UNIQUE INDEX idx_username_directory_skeleton` and contract 0100
    // makes the column mandatory, so the skeleton is a UNIQUENESS CONSTRAINT
    // and `a1ice`/`alice` cannot both exist. D-162 and D-248 are not in
    // conflict once separated: D-162 is about the READ path never folding one
    // spelling onto another, D-248 is about the WRITE path never letting two
    // foldable spellings coexist.
    //
    // The discrimination the old assertion bought is kept, in two pieces.
    const other = userId();
    const otherPair = await registerTestUser(SELF, other);
    expect((await claim("d162_a1ice", other, otherPair)).status).toBe(409);

    // (1) The refusal must not become a resolution. A server that folded the
    // spelling would answer this lookup with `d162_alice`'s row; the
    // read path must simply not know the name.
    const folded = await lookup("d162_a1ice");
    expect(folded.status).toBe(200);
    expect(await folded.json()).toMatchObject({ found: false, username: null });
    // ...and the original is untouched by any of it.
    expect(await (await lookup("d162_alice")).json()).toMatchObject({
      found: true, username: "d162_alice",
    });

    // (2) Two handles that are NOT confusable still both claim and still
    // resolve byte-exactly and separately, so this spec still fails against an
    // implementation that answers `found: false` for everything or that
    // collapses distinct rows.
    const third = userId();
    const thirdPair = await registerTestUser(SELF, third);
    expect((await claim("d162_bravo", third, thirdPair)).status).toBe(200);
    expect(await (await lookup("d162_bravo")).json()).toMatchObject({
      found: true, username: "d162_bravo",
    });
    expect(await (await lookup("d162_alice")).json()).toMatchObject({
      found: true, username: "d162_alice",
    });

    // Case is allowed, but lookup still remains exact and never transforms.
    expect((await lookup("D162_alice")).status).toBe(200);
  });
});
