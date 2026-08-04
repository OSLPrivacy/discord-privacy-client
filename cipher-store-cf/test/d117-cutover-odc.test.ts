/// B0-01 phase 2 — the Observable Delta Contract for closing D-117, executed
/// end-to-end through the Worker's own router (`SELF.fetch`) rather than
/// against the handlers.
///
/// ## Why the router and not the handlers
///
/// `test/blob-manage-token.test.ts` already proves the fetch capability cannot
/// destroy a blob — but it calls `handleUpload` / `handleDelete` directly, so it
/// never observes which HTTP verb or path the Worker actually serves. That blind
/// spot hid a real defect: the Rust client's capability upload sent
/// `POST /v1/blob`, which this Worker answers with 404
/// (`src/index.ts:186` routes upload on PUT only). Four in-repo tests covered
/// that call and every one of them answered from a mock that replied to any
/// verb. Everything below therefore goes through `dispatch`, including
/// `verifyStorageGrant`, rate limiting, D1 and R2.
///
/// ## Why this is not run against the live service
///
/// The Worker that implements this contract has never been deployed. Deploying
/// it is the conductor's call and it is the one irreversible step in this task,
/// so the artifact here is the same four mutants against the same code, one
/// deploy short of production. `crates/ipc/tests/prose_token_bridge_live.rs`
/// holds the live half, and D-117 was reproduced live by hand against
/// `ciphers.oslprivacy.com` on 2026-08-03 — see
/// `plan-test/tasklogs/B0-01-phase2.md`.

import { SELF, env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import { sha256Hex } from "../src/lib/digest.js";
import {
  STORAGE_GRANT_AUDIENCE,
  STORAGE_GRANT_DOMAIN,
  STORAGE_GRANT_SCHEME,
} from "../src/lib/storage-grant.js";
import { GRANT_AUDIENCE as LINK_CREATE_AUDIENCE } from "../src/lib/link-grant.js";

const ORIGIN = "https://cipher.test";
const TTL = "604800";
const BODY = new Uint8Array([7]);

/// The three capabilities of one pointer-derived object. `fetchCap` is what a
/// recipient can derive from the public cover text; `manageCap` is rooted in the
/// sender's send key and is the credential D-117 says must not be substitutable.
const FETCH_CAP = "1".repeat(32);
const ACK_CAP = "2".repeat(32);
const MANAGE_CAP = "3".repeat(32);

function b64u(bytes: Uint8Array): string {
  let out = "";
  for (const byte of bytes) out += String.fromCharCode(byte);
  return btoa(out).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function b64(bytes: Uint8Array): string {
  let out = "";
  for (const byte of bytes) out += String.fromCharCode(byte);
  return btoa(out);
}

function jti(): string {
  return [...crypto.getRandomValues(new Uint8Array(16))]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

let issuer: CryptoKeyPair;

/// Mint a grant exactly the way `keyserver-cf/src/lib/link-grant-issuer.ts`
/// does: a three-claim payload in fixed field order, signed over
/// `DOMAIN || 0x00 || payload`.
async function mintGrant(
  audience: string = STORAGE_GRANT_AUDIENCE,
  expiresAt: number = Math.floor(Date.now() / 1000) + 300,
): Promise<string> {
  const payload = new TextEncoder().encode(
    `{"aud":"${audience}","exp":${expiresAt},"jti":"${jti()}"}`,
  );
  const domain = new TextEncoder().encode(STORAGE_GRANT_DOMAIN);
  const signed = new Uint8Array(domain.byteLength + 1 + payload.byteLength);
  signed.set(domain, 0);
  signed[domain.byteLength] = 0x00;
  signed.set(payload, domain.byteLength + 1);
  const signature = new Uint8Array(
    await crypto.subtle.sign({ name: "Ed25519" }, issuer.privateKey, signed),
  );
  return `${STORAGE_GRANT_SCHEME} ${b64u(payload)}.${b64u(signature)}`;
}

async function uploadHeaders(blobId: string, grant: string | null) {
  const headers: Record<string, string> = {
    "cf-connecting-ip": `198.51.100.${1 + Math.floor(Math.random() * 200)}`,
    "x-osl-ttl-seconds": TTL,
    "x-osl-blob-id": blobId,
    "x-osl-fetch-digest": await sha256Hex(FETCH_CAP),
    "x-osl-ack-digest": await sha256Hex(ACK_CAP),
    "x-osl-manage-digest": await sha256Hex(MANAGE_CAP),
    "x-osl-object-class": "single-ack",
    "x-osl-delivery-tag": "f".repeat(32),
  };
  if (grant !== null) headers.authorization = grant;
  return headers;
}

/// The action under contract: one capability upload carrying a minted grant.
async function upload(
  blobId: string,
  options: { grant?: string | null; omit?: string; override?: Record<string, string> } = {},
): Promise<Response> {
  const grant = options.grant === undefined ? await mintGrant() : options.grant;
  const headers = await uploadHeaders(blobId, grant);
  if (options.omit) delete headers[options.omit];
  Object.assign(headers, options.override ?? {});
  return SELF.fetch(`${ORIGIN}/v1/blob`, { method: "PUT", headers, body: BODY });
}

function read(blobId: string, cap: string): Promise<Response> {
  return SELF.fetch(`${ORIGIN}/v1/blob/${blobId}`, {
    headers: { "cf-connecting-ip": "198.51.100.9", "x-osl-fetch-cap": cap },
  });
}

/// `capHeader` is the header name the credential is presented under, so a
/// FETCH capability can be offered to the DELETE route the way an attacker
/// holding only the cover text would offer it.
function destroy(blobId: string, capHeader: string, cap: string): Promise<Response> {
  return SELF.fetch(`${ORIGIN}/v1/blob/${blobId}`, {
    method: "DELETE",
    headers: { "cf-connecting-ip": "198.51.100.9", [capHeader]: cap },
  });
}

async function storedRowExists(blobId: string): Promise<boolean> {
  return Boolean(
    await env.DB.prepare("SELECT 1 FROM blob_capability_index WHERE blob_id = ?")
      .bind(blobId)
      .first(),
  );
}

let nextId = 0;
function freshId(): string {
  nextId += 1;
  return nextId.toString(16).padStart(32, "0");
}

beforeEach(async () => {
  issuer = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, [
    "sign",
    "verify",
  ])) as CryptoKeyPair;
  const raw = new Uint8Array(
    (await crypto.subtle.exportKey("raw", issuer.publicKey)) as ArrayBuffer,
  );
  env.LINK_GRANT_PUBKEY_B64 = b64(raw);
});

describe("D-117 · reading the cover text must not grant DELETE", () => {
  it("action: a capability upload with a minted grant is 201 and reads back byte-identical", async () => {
    const id = freshId();
    const created = await upload(id);
    expect(created.status).toBe(201);
    await expect(created.json()).resolves.toMatchObject({ id });

    const fetched = await read(id, FETCH_CAP);
    expect(fetched.status).toBe(200);
    expect(new Uint8Array(await fetched.arrayBuffer())).toEqual(BODY);
  });

  it("mutant [1]: a wrong capability neither reads nor uploads", async () => {
    const id = freshId();
    expect((await upload(id)).status).toBe(201);

    // Wrong fetch capability: the refusal is the shared 404, so the route is
    // not an oracle for "this id exists".
    const wrong = `0${FETCH_CAP.slice(1)}`;
    expect((await read(id, wrong)).status).toBe(404);
    // The object is still there, so the 404 above was about the capability.
    expect((await read(id, FETCH_CAP)).status).toBe(200);

    // A capability digest of the wrong shape never reaches storage.
    const malformed = await upload(freshId(), { override: { "x-osl-fetch-digest": "nothex" } });
    expect(malformed.status).toBe(400);
    await expect(malformed.json()).resolves.toMatchObject({ error: "bad_blob_metadata" });
  });

  it("mutant [2]: an absent blob id is refused", async () => {
    const missing = await upload(freshId(), { omit: "x-osl-blob-id" });
    expect(missing.status).toBe(400);
    await expect(missing.json()).resolves.toMatchObject({ error: "bad_blob_metadata" });

    // An id that was never uploaded resolves to the same shared 404.
    expect((await read("9".repeat(32), FETCH_CAP)).status).toBe(404);
  });

  it("mutant [3]: an absent grant cannot create a blob, and neither can an unconfigured verifier", async () => {
    const id = freshId();
    const ungranted = await upload(id, { grant: null });
    expect(ungranted.status).toBe(401);
    await expect(ungranted.json()).resolves.toMatchObject({ error: "grant_required" });
    expect(await storedRowExists(id)).toBe(false);

    // Admission is checked before the body is read, so nothing was persisted.
    expect(await env.PAYLOADS.head(await sha256Hex(FETCH_CAP))).toBeNull();

    // A replayed grant is spent exactly once.
    const grant = await mintGrant();
    const second = freshId();
    expect((await upload(second, { grant })).status).toBe(201);
    const replayed = await upload(freshId(), { grant });
    expect(replayed.status).toBe(401);
    await expect(replayed.json()).resolves.toMatchObject({ error: "grant_replay" });

    // Fail-closed with no verifier key at all.
    env.LINK_GRANT_PUBKEY_B64 = undefined as unknown as string;
    const unconfigured = await upload(freshId(), { grant: null });
    expect(unconfigured.status).toBe(503);
    await expect(unconfigured.json()).resolves.toMatchObject({
      error: "storage_grant_unconfigured",
    });
  });

  it("mutant [4] — D-117 ITSELF: a FETCH capability presented to DELETE must not destroy the object", async () => {
    const id = freshId();
    expect((await upload(id)).status).toBe(201);
    const payloadKey = await sha256Hex(FETCH_CAP);
    expect(await env.PAYLOADS.head(payloadKey)).not.toBeNull();

    // Exactly what a recipient who can read the cover text holds, offered to
    // the destroy route under the manage header. On the deployed legacy Worker
    // the equivalent request returns 204 AND destroys the object; that is
    // D-117, reproduced live on 2026-08-03.
    const asManage = await destroy(id, "x-osl-manage-cap", FETCH_CAP);
    // Burn is oracle-free: a refusal is indistinguishable from an idempotent
    // repeat, so the response code is deliberately NOT the assertion. Survival
    // is.
    expect(asManage.status).toBe(204);

    // The same capability under its own header, in case the route ever grew a
    // second credential surface.
    expect((await destroy(id, "x-osl-fetch-cap", FETCH_CAP)).status).toBe(204);
    // And the receipt capability, which authorises acknowledgement only.
    expect((await destroy(id, "x-osl-manage-cap", ACK_CAP)).status).toBe(204);
    // And no credential at all.
    expect((await destroy(id, "x-osl-object-class", "single-ack")).status).toBe(204);

    // THE ASSERTION. Three unauthorized destroy attempts later, the sender's
    // message is still stored and still readable.
    expect(await storedRowExists(id)).toBe(true);
    expect(await env.PAYLOADS.head(payloadKey)).not.toBeNull();
    const survived = await read(id, FETCH_CAP);
    expect(survived.status).toBe(200);
    expect(new Uint8Array(await survived.arrayBuffer())).toEqual(BODY);

    // The sender's own manage capability still works, so the survival above is
    // capability separation and not a broken delete route.
    expect((await destroy(id, "x-osl-manage-cap", MANAGE_CAP)).status).toBe(204);
    expect(await storedRowExists(id)).toBe(false);
    expect(await env.PAYLOADS.head(payloadKey)).toBeNull();
    expect((await read(id, FETCH_CAP)).status).toBe(404);
  });

  it("POST /v1/blob is not an upload route, so a client that POSTs cannot store anything", async () => {
    // Pins the defect the Rust client carried: `upload_pointer` used POST.
    const id = freshId();
    const headers = await uploadHeaders(id, await mintGrant());
    const posted = await SELF.fetch(`${ORIGIN}/v1/blob`, {
      method: "POST",
      headers,
      body: BODY,
    });
    expect(posted.status).toBe(404);
    expect(await storedRowExists(id)).toBe(false);
  });
});

describe("the cutover blocker: no issuer mints the audience blob upload requires", () => {
  /// `keyserver-cf/src/lib/link-grant-issuer.ts:61,232-236` is the ONLY grant
  /// issuer in the system and it mints `aud: "osl-link-create"`. Blob upload
  /// requires `aud: "osl-blob-store"` (`src/lib/storage-grant.ts:12`). So
  /// flipping `LINK_GRANT_ENABLED` and installing `LINK_GRANT_PUBKEY_B64` is
  /// NOT sufficient: every capability upload would still be refused.
  ///
  /// This test exists to make that a red line rather than a paragraph. Do not
  /// make it pass by changing either audience — the split is what stops a
  /// link-creation grant from becoming blob-upload authority. Make it pass by
  /// teaching the keyserver's issuer to mint the blob-store audience, then
  /// delete this test and assert 201.
  it("a grant carrying the issuer's audience is refused by blob upload", async () => {
    expect(LINK_CREATE_AUDIENCE).toBe("osl-link-create");
    expect(STORAGE_GRANT_AUDIENCE).toBe("osl-blob-store");
    expect(LINK_CREATE_AUDIENCE).not.toBe(STORAGE_GRANT_AUDIENCE);

    const issued = await mintGrant(LINK_CREATE_AUDIENCE);
    const refused = await upload(freshId(), { grant: issued });
    expect(refused.status).toBe(401);
    await expect(refused.json()).resolves.toMatchObject({ error: "grant_audience" });
  });
});
