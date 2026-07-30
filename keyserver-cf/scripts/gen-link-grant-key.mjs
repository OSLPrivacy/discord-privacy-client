#!/usr/bin/env node
/**
 * Generate the view-once link-grant issuer keypair.
 *
 *   node scripts/gen-link-grant-key.mjs
 *
 * Prints two values and writes nothing to disk:
 *
 *   LINK_GRANT_SECRET_B64  base64 PKCS#8 Ed25519 private key.
 *                          Keyserver Worker secret ONLY.
 *   LINK_GRANT_PUBKEY_B64  base64 raw 32-byte public key.
 *                          Goes on BOTH Workers.
 *
 * Custody rules, in order of how badly each one bites:
 *
 *   1. The private half is a link-creation capability for the whole
 *      lane. Anyone holding it can mint unlimited grants and turn the
 *      cipher-store into an open file host. It belongs in
 *      `wrangler secret put` and nowhere else -- not in .env, not in
 *      the repo, not in a password-manager note shared with a build
 *      machine, not pasted into a chat.
 *   2. It is generated here and never leaves this process except onto
 *      the terminal. Nothing writes it to a file, because the fastest
 *      way to leak a key is to give it a filename.
 *   3. Rotation is cheap and non-breaking in one direction only:
 *      install the new public key on the cipher-store FIRST, then the
 *      new secret here. Doing it the other way round refuses every
 *      link creation in between. There is no dual-key window -- the
 *      verifier holds exactly one public key -- so rotate at a quiet
 *      moment and accept a few seconds of `grant_signature` refusals.
 *   4. If you suspect the private half leaked, rotate immediately. A
 *      leaked grant issuer is not an information disclosure -- grants
 *      are anonymous and carry nothing -- it is an abuse capability.
 *
 * Pipe safety: this prints secrets to stdout. Do not redirect it into a
 * file, a log, or a shell that records history with output.
 */

import { generateKeyPairSync } from "node:crypto";

const { publicKey, privateKey } = generateKeyPairSync("ed25519");

// PKCS#8 DER, exactly what crypto.subtle.importKey("pkcs8", ...) wants.
const secret = privateKey.export({ type: "pkcs8", format: "der" });
// SPKI DER for Ed25519 is a fixed 12-byte header followed by the raw
// 32-byte key; the verifier imports "raw", so strip the header.
const spki = publicKey.export({ type: "spki", format: "der" });
const raw = spki.subarray(spki.length - 32);

console.log("");
console.log("  LINK_GRANT_SECRET_B64 (keyserver-cf secret, PRIVATE):");
console.log("  " + secret.toString("base64"));
console.log("");
console.log("  LINK_GRANT_PUBKEY_B64 (both Workers, public):");
console.log("  " + raw.toString("base64"));
console.log("");
console.log("  Install, in this order:");
console.log("    cd cipher-store-cf && npx wrangler secret put LINK_GRANT_PUBKEY_B64");
console.log("    cd keyserver-cf   && npx wrangler secret put LINK_GRANT_PUBKEY_B64");
console.log("    cd keyserver-cf   && npx wrangler secret put LINK_GRANT_SECRET_B64");
console.log("");
