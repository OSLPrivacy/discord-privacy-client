/// D81 — "nobody can identify anybody from server stuff", username-directory
/// half.
///
/// The directory is public and enumerable on purpose: a handle is meant to be
/// resolvable by strangers. What must not leak is THE PAIRING -- "this
/// address wanted this person, at this second" -- and that pairing leaks in
/// two places that have nothing to do with the database:
///
///   1. The URL. Cloudflare records `ClientRequestURI` for a proxied zone by
///      default and does not record headers or bodies. A handle in the path
///      is therefore retained next to `ClientIP`, and no OSL setting turns
///      that off -- `[observability] enabled = false` governs this Worker's
///      own logs, not the zone's HTTP request data.
///   2. The response length. `friend_code` is a signed key bundle that ranges
///      over roughly 8 KiB, so an unpadded 200 tells a passive observer which
///      handle resolved, not merely that one did. A 404 tells them it did
///      not.
///
/// These tests pin both. They do NOT claim the lookup is k-anonymous: it is
/// not, and the audit records that as an open defect rather than a property.

import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { USERNAME_LOOKUP_RESPONSE_BYTES } from "../../src/endpoints/usernames.js";
import { usernameClaimMessage } from "../../src/lib/username.js";
import {
  base64Encode,
  registerTestUser,
  signEd25519,
} from "./helpers.js";

let sequence = 0;
const userId = () => `d81-username-${Date.now()}-${sequence++}`;

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

async function claim(
  username: string,
  uid: string,
  pair: { publicKeyB64: string; signingKey: CryptoKey },
  invite: string,
): Promise<Response> {
  const request_id = b64url(crypto.getRandomValues(new Uint8Array(32)));
  const timestamp_ms = Date.now();
  const message = usernameClaimMessage({
    username,
    user_id: uid,
    friend_code: invite,
    request_id,
    timestamp_ms,
  });
  const signature_b64 = await signEd25519(pair.signingKey, message);
  return SELF.fetch("http://test/v1/usernames/claim", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.200" },
    body: JSON.stringify({
      username,
      user_id: uid,
      friend_code: invite,
      request_id,
      timestamp_ms,
      signature_b64,
    }),
  });
}

async function lookup(username: string, ip: string): Promise<Response> {
  return SELF.fetch("http://test/v1/usernames/lookup", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": ip },
    body: JSON.stringify({ username }),
  });
}

async function claimed(username: string): Promise<void> {
  const uid = userId();
  const pair = await registerTestUser(SELF, uid);
  const invite = await friendCode(uid, pair);
  expect((await claim(username, uid, pair, invite)).status).toBe(200);
}

describe("D81 — a username lookup never puts the handle in the URL", () => {
  it("has no GET route that carries a handle in the path", async () => {
    await claimed("path_probe_user");

    // The route this replaced answered 200 here. If it ever comes back, the
    // handle starts being written to retained platform state again.
    const viaPath = await SELF.fetch("http://test/v1/usernames/path_probe_user", {
      headers: { "cf-connecting-ip": "203.0.113.201" },
    });
    expect(viaPath.status).toBe(404);
  });

  it("does not accept the handle from a query string either", async () => {
    await claimed("query_probe_user");

    // A query string is part of `ClientRequestURI`, so a `?username=` fallback
    // would be the same leak wearing a different hat.
    const viaQuery = await SELF.fetch(
      "http://test/v1/usernames/lookup?username=query_probe_user",
      {
        method: "POST",
        headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.202" },
        body: JSON.stringify({}),
      },
    );
    expect(viaQuery.status).toBe(400);
  });

  it("resolves a handle carried in the request body", async () => {
    await claimed("body_probe_user");

    const response = await lookup("body_probe_user", "203.0.113.203");
    expect(response.status).toBe(200);
    const body = (await response.json()) as { found: boolean; username: string | null };
    expect(body.found).toBe(true);
    expect(body.username).toBe("body_probe_user");
  });
});

describe("D81 — a lookup hit is indistinguishable from a miss by size or status", () => {
  it("answers a hit and a miss with the same status, headers and byte length", async () => {
    await claimed("present_user");

    const hit = await lookup("present_user", "203.0.113.204");
    const miss = await lookup("absent_user", "203.0.113.205");

    expect(hit.status).toBe(200);
    expect(miss.status).toBe(200);
    expect(hit.headers.get("content-type")).toBe(miss.headers.get("content-type"));
    expect(hit.headers.get("cache-control")).toBe(miss.headers.get("cache-control"));

    const hitBytes = new TextEncoder().encode(await hit.text()).length;
    const missBytes = new TextEncoder().encode(await miss.text()).length;
    expect(hitBytes).toBe(USERNAME_LOOKUP_RESPONSE_BYTES);
    expect(missBytes).toBe(USERNAME_LOOKUP_RESPONSE_BYTES);
  });

  it("answers two hits with different-sized friend codes at the same length", async () => {
    // Both handles are real, both resolve, and their friend codes differ in
    // length because the identities differ. Padding to a constant is the only
    // thing standing between that and "which person did they ask about".
    await claimed("short_one");
    await claimed("a_much_longer_handle_here");

    const first = await lookup("short_one", "203.0.113.206");
    const second = await lookup("a_much_longer_handle_here", "203.0.113.207");

    const firstBody = await first.text();
    const secondBody = await second.text();
    expect(JSON.parse(firstBody).found).toBe(true);
    expect(JSON.parse(secondBody).found).toBe(true);
    expect(new TextEncoder().encode(firstBody).length)
      .toBe(new TextEncoder().encode(secondBody).length);
  });

  it("pads every answer to the same length regardless of handle length", async () => {
    // A pad computed from anything other than the final encoded size (say,
    // a fixed number of characters) would track the handle's length.
    const lengths = new Set<number>();
    for (const [index, name] of ["abc", "abcdefghij", "abcdefghijklmnopqrstuvwxyz1234"].entries()) {
      const response = await lookup(name, `203.0.113.${210 + index}`);
      expect(response.status).toBe(200);
      lengths.add(new TextEncoder().encode(await response.text()).length);
    }
    expect([...lengths]).toEqual([USERNAME_LOOKUP_RESPONSE_BYTES]);
  });
});
