// D-248 — the confusable-username defence must actually run.
//
// `username_directory.username_skeleton` exists so that two visually
// confusable handles collide on `idx_username_directory_skeleton` and the
// second claim is refused. The claim path used to write the RAW username into
// that column (`SELECT ?1, ?1, ?1, ...`), so the skeleton could never collide
// with anything the raw name did not already collide with and the column was
// decorative.
//
// WHY THESE PAIRS AND NOT `pаypal` (Cyrillic а). The shipping grammar is
// `^[a-z0-9](?:[a-z0-9_]{1,28}[a-z0-9])?$` — ASCII only, validate-don't-
// transform (D-162). A Cyrillic handle is rejected at the door with a 400, so
// it is NOT the reachable attack. The reachable attack is the ASCII-internal
// confusable set, which UTS #39 folds just as hard:
//
//   skeleton("paypal")  == skeleton("paypa1")  == "paypal"   (1 -> l)
//   skeleton("michael") == skeleton("michae1") == "rnichael" (m -> rn, 1 -> l)
//   skeleton("bob")     == skeleton("b0b")     == "bOb"      (0 -> O)
//
// Every handle used here is claimable under the shipping grammar, so each
// assertion is an assertion about production behaviour and not about a
// hypothetical future one.

import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { usernameClaimMessage } from "../../src/lib/username.js";
import { canonicalUnregisterBytes } from "../../src/lib/canonical.js";
import { buildRegMsg, buildRotMsg } from "../../src/lib/signed-request.js";
import {
  base64Encode,
  generateEd25519Pair,
  registerTestUser,
  signEd25519,
  STUB_MLKEM_PUB_B64,
  STUB_RATCHET_PUB_B64,
  STUB_X25519_PUB_B64,
} from "./helpers.js";

let sequence = 0;
const userId = () => `d248-user-${Date.now()}-${sequence++}`;

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
): Promise<Response> {
  const friend_code = await friendCode(uid, pair);
  const request_id = b64url(crypto.getRandomValues(new Uint8Array(32)));
  const timestamp_ms = Date.now();
  const signature_b64 = await signEd25519(pair.signingKey, usernameClaimMessage({
    username, user_id: uid, friend_code, request_id, timestamp_ms,
  }));
  return SELF.fetch("http://test/v1/usernames/claim", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "cf-connecting-ip": `198.51.100.${(sequence++ % 240) + 1}`,
    },
    body: JSON.stringify({ username, user_id: uid, friend_code, request_id, timestamp_ms, signature_b64 }),
  });
}

async function rotateKeys(uid: string, current: { publicKeyB64: string; signingKey: CryptoKey }) {
  const next = await generateEd25519Pair();
  const fields = {
    user_id: uid,
    ik_x25519_pub: STUB_X25519_PUB_B64,
    ik_ed25519_pub: next.publicKeyB64,
    ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
    ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
  };
  const registration_sig = await signEd25519(next.signingKey, buildRegMsg(fields));
  const prev_sig = await signEd25519(current.signingKey, buildRotMsg({
    user_id: uid,
    prev_ik_ed25519_pub: current.publicKeyB64,
    new_ik_x25519_pub: fields.ik_x25519_pub,
    new_ik_ed25519_pub: fields.ik_ed25519_pub,
    new_ik_mlkem768_pub: fields.ik_mlkem768_pub,
    new_ik_ratchet_initial_pub: fields.ik_ratchet_initial_pub,
  }));
  return SELF.fetch("http://test/v1/register", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.60" },
    body: JSON.stringify({
      ...fields,
      registration_sig,
      rotation: { prev_ik_ed25519_pub: current.publicKeyB64, prev_sig },
    }),
  });
}

async function unregister(uid: string, pair: { signingKey: CryptoKey }) {
  const timestamp_ms = Date.now();
  const signature_b64 = await signEd25519(
    pair.signingKey,
    canonicalUnregisterBytes({ user_id: uid, timestamp_ms }),
  );
  return SELF.fetch(`http://test/v1/pubkeys/${encodeURIComponent(uid)}`, {
    method: "DELETE",
    headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.61" },
    body: JSON.stringify({ signature_b64, timestamp_ms }),
  });
}

/// Read the row back through D1 so the assertion is about the STORED state,
/// not about the response body. The whole defect was that a stored column did
/// not hold what its name promised.
async function storedSkeleton(username: string): Promise<string | null> {
  const { env } = await import("cloudflare:test");
  const row = await (env as { DB: D1Database }).DB
    .prepare("SELECT username_skeleton FROM username_directory WHERE username = ?")
    .bind(username)
    .first<{ username_skeleton: string | null }>();
  return row?.username_skeleton ?? null;
}

async function storedTombstoneSkeleton(username: string): Promise<string | null> {
  const { env } = await import("cloudflare:test");
  const row = await (env as { DB: D1Database }).DB
    .prepare("SELECT skeleton FROM username_tombstones WHERE username = ?")
    .bind(username)
    .first<{ skeleton: string | null }>();
  return row?.skeleton ?? null;
}

describe("D-248 · the confusable-username defence", () => {
  it("refuses a confusable twin of a live username", async () => {
    const one = userId();
    const two = userId();
    const pairOne = await registerTestUser(SELF, one);
    const pairTwo = await registerTestUser(SELF, two);
    expect((await claim("paypal", one, pairOne)).status).toBe(200);
    // `paypa1` and `paypal` are a single UTS #39 skeleton apart. Allowing this
    // is impersonation at the moment a user decides whom to trust.
    expect((await claim("paypa1", two, pairTwo)).status).toBe(409);
  });

  it("refuses the twin in the other direction too", async () => {
    const one = userId();
    const two = userId();
    const pairOne = await registerTestUser(SELF, one);
    const pairTwo = await registerTestUser(SELF, two);
    expect((await claim("michae1", one, pairOne)).status).toBe(200);
    expect((await claim("michael", two, pairTwo)).status).toBe(409);
  });

  it("stores the UTS #39 skeleton, not the raw name", async () => {
    const uid = userId();
    const pair = await registerTestUser(SELF, uid);
    expect((await claim("stored_michael", uid, pair)).status).toBe(200);
    // The one assertion that would have caught D-248 on the day it landed.
    expect(await storedSkeleton("stored_michael")).toBe("stored_rnichael");
    expect(await storedSkeleton("stored_michael")).not.toBe("stored_michael");
  });

  // D-248b. Zero-for-o survives the raw UTS #39 skeleton (`0` folds to the
  // UPPERCASE prototype `O`, `o` is its own), so this pair is only refused
  // because `usernameSkeleton` case-folds the skeleton with the artifact's own
  // normalizer. Without that line this test is the one that goes red.
  it("refuses zero-for-o, the commonest ASCII homograph", async () => {
    const one = userId();
    const two = userId();
    const pairOne = await registerTestUser(SELF, one);
    const pairTwo = await registerTestUser(SELF, two);
    expect((await claim("support_desk", one, pairOne)).status).toBe(200);
    expect((await claim("supp0rt_desk", two, pairTwo)).status).toBe(409);
  });

  it("still allows two genuinely distinct names", async () => {
    const one = userId();
    const two = userId();
    const pairOne = await registerTestUser(SELF, one);
    const pairTwo = await registerTestUser(SELF, two);
    expect((await claim("distinct_one", one, pairOne)).status).toBe(200);
    expect((await claim("wholly_other", two, pairTwo)).status).toBe(200);
  });

  it("lets the same identity re-claim its own name (idempotent refresh)", async () => {
    const uid = userId();
    const pair = await registerTestUser(SELF, uid);
    expect((await claim("selfsame_name", uid, pair)).status).toBe(200);
    expect((await claim("selfsame_name", uid, pair)).status).toBe(200);
    expect(await storedSkeleton("selfsame_name")).toBe("selfsarne_narne");
  });

  // A row written by the pre-fix generation carries the raw name in the
  // skeleton column. If a refresh left it there, D-248 would survive its own
  // fix for every identity that already exists — so the upsert branch rewrites
  // both derived columns.
  it("repairs a legacy raw skeleton when the owner refreshes its claim", async () => {
    const uid = userId();
    const pair = await registerTestUser(SELF, uid);
    const { env } = await import("cloudflare:test");
    await (env as { DB: D1Database }).DB.prepare(
      `INSERT INTO username_directory
         (username, username_skeleton, display_username, user_id, friend_code, claimed_at, updated_at)
       VALUES (?1, ?1, ?1, ?2, 'OSLFR1.legacy', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')`,
    ).bind("legacy_michael", uid).run();
    expect(await storedSkeleton("legacy_michael")).toBe("legacy_michael");

    expect((await claim("legacy_michael", uid, pair)).status).toBe(200);
    expect(await storedSkeleton("legacy_michael")).toBe("legacy_rnichael");

    // and the repair is what makes the confusable refusable
    const other = userId();
    const otherPair = await registerTestUser(SELF, other);
    expect((await claim("legacy_michae1", other, otherPair)).status).toBe(409);
  });

  // ── the other two production writers of the skeleton ────────────────────
  // Neither computes a skeleton: both COPY `username_directory.username_skeleton`
  // into `username_tombstones.skeleton`. So they are only correct if the claim
  // path is, and a raw skeleton in the directory silently becomes a raw
  // skeleton in the tombstone — a retired name whose confusables are reusable.

  // Every handle below is chosen so that skeleton != raw name (`michael` ->
  // `rnichael`). A pair like `paypal`/`paypa1` would NOT pin these writers:
  // `paypal` is its own skeleton, so a writer storing the raw name looks
  // identical to one storing the skeleton and the mutant survives. Measured,
  // not assumed — M3 and M4 survived exactly that mistake.

  it("keeps a rotated-away name's confusables retired (src/lib/db.ts rotateUserKeys)", async () => {
    const one = userId();
    const two = userId();
    const pairOne = await registerTestUser(SELF, one);
    const pairTwo = await registerTestUser(SELF, two);
    expect((await claim("rotate_michael", one, pairOne)).status).toBe(200);
    expect((await rotateKeys(one, pairOne)).status).toBe(200);
    // This writer runs BEFORE the delete trigger and the trigger's insert is
    // OR IGNORE, so the stored value is this writer's and nothing else's.
    expect(await storedTombstoneSkeleton("rotate_michael")).toBe("rotate_rnichael");
    // No live row remains, so the refusal is the tombstone skeleton's doing.
    expect((await claim("rotate_michae1", two, pairTwo)).status).toBe(409);
  });

  it("keeps an unregistered name's confusables retired (src/lib/db.ts unregisterUserIfCurrent)", async () => {
    const one = userId();
    const two = userId();
    const pairOne = await registerTestUser(SELF, one);
    const pairTwo = await registerTestUser(SELF, two);
    expect((await claim("delete_michael", one, pairOne)).status).toBe(200);
    expect((await unregister(one, pairOne)).status).toBe(200);
    expect(await storedTombstoneSkeleton("delete_michael")).toBe("delete_rnichael");
    expect((await claim("delete_michae1", two, pairTwo)).status).toBe(409);
  });

  it("keeps a renamed-away name's confusables retired (the BEFORE DELETE trigger)", async () => {
    const one = userId();
    const two = userId();
    const pairOne = await registerTestUser(SELF, one);
    const pairTwo = await registerTestUser(SELF, two);
    expect((await claim("rename_michael", one, pairOne)).status).toBe(200);
    expect((await claim("rename_elsewhere", one, pairOne)).status).toBe(200);
    // The rename path has no explicit tombstone writer; migration 0038's
    // BEFORE DELETE trigger is the only thing that retires this name.
    expect(await storedTombstoneSkeleton("rename_michael")).toBe("renarne_rnichael");
    expect((await claim("rename_michae1", two, pairTwo)).status).toBe(409);
  });
});
