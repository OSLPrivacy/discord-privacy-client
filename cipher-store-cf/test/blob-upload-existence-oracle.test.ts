/// D-255 — the blob upload route must not tell a caller whether a blob id
/// already exists.
///
/// D81 and t1-15 closed this leak class on the fetch route by collapsing every
/// negative outcome onto one indistinguishable response ("a malformed id must
/// look exactly like an absent id"). `cc619a55e` re-opened it on upload with a
/// `409 blob_id_collision` that a fresh id never produces.
///
/// The probe below holds NO authority over the id it names. `PUT /v1/blob` is
/// gated by `verifyStorageGrant`, and that grant is anonymous, single-use, and
/// carries exactly three claims — `aud`, `exp`, `jti`. None of them names a
/// blob, so holding one authorizes *an upload*, never *a lookup*. Blob ids are
/// client-derived pointers, so an observer who has seen a pointer in a cover
/// text can use a status-code difference to confirm it names real stored
/// ciphertext, with no capability for that blob at all.
///
/// The grant is one-time, so each probe costs one grant — a rate cost, not an
/// authorization. The signal is what has to go, not its price.

import { SELF, env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import { MAX_BLOB_BYTES } from "../src/endpoints/blob.js";
import { sha256Hex } from "../src/lib/digest.js";
import {
  STORAGE_GRANT_DOMAIN,
  STORAGE_GRANT_SCHEME,
} from "../src/lib/storage-grant.js";

const ORIGIN = "https://cipher.test";
const PROBE_BODY = new Uint8Array([7]);
const PROBE_TTL = 3600;

/// The victim row's window is deliberately far from the probe's, so a response
/// that echoed the *stored* expiry instead of the probe's own would be caught.
const VICTIM_TTL = 604_800;

const EXISTING_ID = "a".repeat(32);
const UNUSED_ID = "b".repeat(32);

function b64u(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function installGrantIssuer(): Promise<CryptoKeyPair> {
  const pair = (await crypto.subtle.generateKey(
    { name: "Ed25519" },
    true,
    ["sign", "verify"],
  )) as CryptoKeyPair;
  const raw = new Uint8Array(
    (await crypto.subtle.exportKey("raw", pair.publicKey)) as ArrayBuffer,
  );
  let binary = "";
  for (const byte of raw) binary += String.fromCharCode(byte);
  env.LINK_GRANT_PUBKEY_B64 = btoa(binary);
  return pair;
}

/// A fresh anonymous admission grant. Nothing in the signed payload names a
/// blob id — that is the whole point of the finding.
async function freshGrant(pair: CryptoKeyPair): Promise<string> {
  const jti = [...crypto.getRandomValues(new Uint8Array(16))]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  const payload = new TextEncoder().encode(
    JSON.stringify({
      aud: "osl-blob-store",
      exp: Math.floor(Date.now() / 1000) + 120,
      jti,
    }),
  );
  const domain = new TextEncoder().encode(STORAGE_GRANT_DOMAIN);
  const signed = new Uint8Array(domain.byteLength + 1 + payload.byteLength);
  signed.set(domain, 0);
  signed[domain.byteLength] = 0;
  signed.set(payload, domain.byteLength + 1);
  const signature = new Uint8Array(
    await crypto.subtle.sign({ name: "Ed25519" }, pair.privateKey, signed),
  );
  return `${STORAGE_GRANT_SCHEME} ${b64u(payload)}.${b64u(signature)}`;
}

/// Capability digests the prober invented. They match nothing the victim holds.
async function probeUpload(pair: CryptoKeyPair, blobId: string): Promise<Response> {
  return SELF.fetch(`${ORIGIN}/v1/blob`, {
    method: "PUT",
    headers: {
      authorization: await freshGrant(pair),
      "cf-connecting-ip": "198.51.100.44",
      "content-type": "application/octet-stream",
      "x-osl-ttl-seconds": String(PROBE_TTL),
      "x-osl-expiry-mode": "absolute",
      "x-osl-blob-id": blobId,
      "x-osl-fetch-digest": await sha256Hex("probe-fetch"),
      "x-osl-ack-digest": await sha256Hex("probe-ack"),
      "x-osl-manage-digest": await sha256Hex("probe-manage"),
      "x-osl-delivery-tag": "9".repeat(32),
      "x-osl-object-class": "single-ack",
    },
    body: PROBE_BODY,
  });
}

interface Observation {
  status: number;
  headers: [string, string][];
  /// Body with the two fields the *caller itself* supplied or can compute
  /// masked out: the echoed id and the probe's own expiry. Everything else
  /// must be byte-identical, or the route is answering about stored state.
  maskedBody: string;
  expiresAt: unknown;
}

async function observe(response: Response, blobId: string): Promise<Observation> {
  const text = await response.text();
  let expiresAt: unknown;
  try {
    expiresAt = (JSON.parse(text) as { expires_at?: unknown }).expires_at;
  } catch {
    expiresAt = undefined;
  }
  return {
    status: response.status,
    headers: [...response.headers.entries()].sort(([a], [b]) => a.localeCompare(b)),
    maskedBody: text
      .replaceAll(blobId, "<caller-supplied-id>")
      .replace(/"expires_at"\s*:\s*\d+/, '"expires_at":<caller-chosen-window>'),
    expiresAt,
  };
}

async function seedVictimRow(): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await env.DB.prepare(
    `INSERT INTO blob_capability_index (
      blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
      manage_digest_sha256_hex, object_class, pool, delivery_tag,
      size_bytes, expires_at, created_at
    ) VALUES (?, ?, ?, ?, 'multi-fetch', 'undelivered', ?, ?, ?, ?)`,
  ).bind(
    EXISTING_ID,
    await sha256Hex("victim-fetch"),
    await sha256Hex("victim-ack"),
    await sha256Hex("victim-manage"),
    "1".repeat(32),
    MAX_BLOB_BYTES,
    now + VICTIM_TTL,
    now,
  ).run();
}

describe("D-255 blob upload is not an existence oracle", () => {
  let issuer: CryptoKeyPair;

  beforeEach(async () => {
    issuer = await installGrantIssuer();
    await seedVictimRow();
  });

  it("answers a taken blob id exactly as it answers an unused one", async () => {
    const before = Math.floor(Date.now() / 1000);
    const taken = await observe(await probeUpload(issuer, EXISTING_ID), EXISTING_ID);
    const unused = await observe(await probeUpload(issuer, UNUSED_ID), UNUSED_ID);
    const after = Math.floor(Date.now() / 1000);

    // The gate itself must be reachable: a grant-bearing probe is not being
    // turned away by admission, rate limiting, or metadata validation.
    expect(unused.status).toBeLessThan(400);

    // The only signal the route may carry is one the caller already had.
    expect(taken.status).toBe(unused.status);
    expect(taken.headers).toEqual(unused.headers);
    expect(taken.maskedBody).toBe(unused.maskedBody);

    // Masking may not be doing the hiding: both windows are the prober's own,
    // never the seven-day window stored against the id it probed.
    for (const observation of [taken, unused]) {
      expect(observation.expiresAt).toBeGreaterThanOrEqual(before + PROBE_TTL);
      expect(observation.expiresAt).toBeLessThanOrEqual(after + PROBE_TTL);
    }
  });

  it("leaves the row it collided with exactly as it found it", async () => {
    const before = await env.DB.prepare(
      "SELECT * FROM blob_capability_index WHERE blob_id = ?",
    ).bind(EXISTING_ID).first();

    await probeUpload(issuer, EXISTING_ID);

    const after = await env.DB.prepare(
      "SELECT * FROM blob_capability_index WHERE blob_id = ?",
    ).bind(EXISTING_ID).first();
    expect(after).toEqual(before);

    // Nor may the probe plant bytes under a digest it chose while naming an id
    // it does not own.
    expect(await env.PAYLOADS.head(await sha256Hex("probe-fetch"))).toBeNull();
  });
});
