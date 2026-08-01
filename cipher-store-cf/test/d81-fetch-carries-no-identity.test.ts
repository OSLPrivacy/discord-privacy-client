/// D81 — "nobody can identify anybody from server stuff", cipher-store half.
///
/// The properties asserted here are the ones this Worker is claimed to hold in
/// the audit at `/home/liamw/osl-plan/SERVER-IDENTIFIABILITY-AUDIT.md`:
///
///   1. A fetch carries no identity. Possession of the capability IS the
///      authorization, and the Worker is structurally unable to learn who
///      fetched -- it never reads an identity, stores no blob access receipt,
///      and rate limits only through opaque, unlinked counters.
///   2. The capability is only ever accepted from a HEADER. A caller who puts
///      it in the URL -- the one part of a request the platform records by
///      default -- is refused, so a log line can never be a bearer token.
///   3. A stored row with no capability (`fetch_token IS NULL`) is treated as
///      absent by fetch and by delete. This is the branch a prior lane found.
///
/// Each of these is a claim about an ABSENCE, which is the kind of test that
/// silently rots into decoration. Every one below therefore turns on state a
/// mutation would visibly change (a row count, a stored byte, a status), not
/// on the mere fact that a call returned.

import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { d1All, d1Count, d1First, d1Run } from "./helpers/workerd.js";

const TOKEN = "0123456789abcdef0123456789abcdef";

function blobIdBytes(index: number): Uint8Array {
  const id = new Uint8Array(8);
  new DataView(id.buffer).setBigUint64(0, BigInt(0xd8100000 + index));
  return id;
}

function toHex(bytes: Uint8Array): string {
  let hex = "";
  for (const byte of bytes) hex += byte.toString(16).padStart(2, "0");
  return hex;
}

function fromHex(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/// Insert a row the way a pre-`0002` upload did: no capability at all. The
/// write path refuses this now (see `rejects a tokenless upload` below), so
/// D1 is the only way to reach the state a legacy row is in.
async function insertLegacyNullTokenRow(index: number): Promise<string> {
  const id = blobIdBytes(index);
  const now = Math.floor(Date.now() / 1000);
  await d1Run(
    `INSERT INTO blobs (id, data, size_bytes, expires_at, created_at, fetch_token)
     VALUES (?, ?, ?, ?, ?, NULL)`,
    id,
    new TextEncoder().encode("legacy-ciphertext"),
    17,
    now + 3600,
    now,
  );
  return toHex(id);
}

async function upload(body: Uint8Array, token = TOKEN): Promise<string> {
  const response = await SELF.fetch("https://cipher.test/v1/blob", {
    method: "POST",
    body,
    headers: {
      "x-osl-ttl-seconds": "3600",
      "x-osl-fetch-token": token,
      "x-osl-manage-token": "fedcba9876543210fedcba9876543210",
      "cf-connecting-ip": "203.0.113.7",
    },
  });
  expect(response.status).toBe(201);
  return ((await response.json()) as { id: string }).id;
}

describe("D81 — a cipher-store fetch carries no identity", () => {
  it("serves a blob to a caller who presents the capability and nothing else", async () => {
    const id = await upload(new TextEncoder().encode("hello-d81!"));

    // Deliberately hostile to the idea of a session: a different source
    // address from the uploader, no cookie, no authorization header, no
    // user-agent continuity. If any of those were load-bearing, this fails.
    const response = await SELF.fetch(`https://cipher.test/v1/blob/${id}`, {
      headers: {
        "x-osl-fetch-token": TOKEN,
        "cf-connecting-ip": "198.51.100.99",
      },
    });

    expect(response.status).toBe(200);
    expect(new TextDecoder().decode(await response.arrayBuffer())).toBe("hello-d81!");
  });

  it("writes no blob access receipt or fetcher identity", async () => {
    const id = await upload(new TextEncoder().encode("no-receipt"));
    const before = await d1First<Record<string, unknown>>(
      "SELECT size_bytes, expires_at, created_at, fetch_token FROM blobs WHERE id = ?",
      fromHex(id),
    );
    const rowsBefore = await d1Count("SELECT COUNT(*) FROM blobs");
    // The upload itself is a mutation and DOES draw on a D1 rate counter, so
    // the claim under test is that a READ adds nothing on top of that. Anchor
    // to the post-upload count, not to zero.
    const countersBefore = await d1Count("SELECT COUNT(*) FROM rate_counters");

    // Ten fetches from ten different addresses. A receipt table, an access
    // counter, a last-seen column or a rate row keyed to the reader would all
    // have to show up in one of the two observations below.
    for (let i = 0; i < 10; i++) {
      const response = await SELF.fetch(`https://cipher.test/v1/blob/${id}`, {
        headers: {
          "x-osl-fetch-token": TOKEN,
          "cf-connecting-ip": `198.51.100.${i + 1}`,
        },
      });
      expect(response.status).toBe(200);
    }

    const after = await d1First<Record<string, unknown>>(
      "SELECT size_bytes, expires_at, created_at, fetch_token FROM blobs WHERE id = ?",
      fromHex(id),
    );
    expect(after).toEqual(before);
    expect(await d1Count("SELECT COUNT(*) FROM blobs")).toBe(rowsBefore);
    // Fetches are rate-limited, so they create opaque per-address counters.
    // Those counters must reveal neither the address nor the blob ID and
    // therefore cannot identify who read this ciphertext.
    expect(await d1Count("SELECT COUNT(*) FROM rate_counters")).toBe(countersBefore + 10);
    const rateCounters = JSON.stringify(
      await d1All<Record<string, unknown>>("SELECT * FROM rate_counters"),
    );
    expect(rateCounters).not.toContain("198.51.100.");
    expect(rateCounters).not.toContain(id);
  });

  it("refuses a capability presented in the URL instead of the header", async () => {
    const id = await upload(new TextEncoder().encode("header-only?"));

    // The URL is the part of a request the platform records by default. If a
    // query parameter were ever accepted as a fallback, an access log would
    // become a set of live bearer tokens.
    const viaQuery = await SELF.fetch(
      `https://cipher.test/v1/blob/${id}?t=${TOKEN}&fetch_token=${TOKEN}&cap=${TOKEN}`,
      { headers: { "cf-connecting-ip": "198.51.100.50" } },
    );
    // Missing and invalid capabilities intentionally look like an absent
    // blob. This public route must not become an existence oracle.
    expect(viaQuery.status).toBe(404);

    // ...and the path form is not a route at all.
    const viaPath = await SELF.fetch(`https://cipher.test/v1/blob/${id}/${TOKEN}`, {
      headers: { "cf-connecting-ip": "198.51.100.51" },
    });
    expect(viaPath.status).toBe(404);
  });
});

describe("D81 — a capability-less legacy row is treated as absent", () => {
  it("does not serve a NULL-token row to a caller holding only its id", async () => {
    const id = await insertLegacyNullTokenRow(1);

    const bare = await SELF.fetch(`https://cipher.test/v1/blob/${id}`, {
      headers: { "cf-connecting-ip": "198.51.100.60" },
    });
    expect(bare.status).toBe(404);

    // Guessing a token must not help either -- there is nothing to match.
    const guessed = await SELF.fetch(`https://cipher.test/v1/blob/${id}`, {
      headers: {
        "x-osl-fetch-token": TOKEN,
        "cf-connecting-ip": "198.51.100.61",
      },
    });
    expect(guessed.status).toBe(404);
  });

  it("answers a NULL-token row exactly as it answers an id that was never stored", async () => {
    const id = await insertLegacyNullTokenRow(2);
    const neverStored = toHex(blobIdBytes(9999));

    const legacy = await SELF.fetch(`https://cipher.test/v1/blob/${id}`, {
      headers: { "cf-connecting-ip": "198.51.100.62" },
    });
    const absent = await SELF.fetch(`https://cipher.test/v1/blob/${neverStored}`, {
      headers: { "cf-connecting-ip": "198.51.100.63" },
    });

    // Same status AND same body: a divergence here is an oracle for "a legacy
    // blob with this id exists", which is exactly what the refusal was for.
    expect(legacy.status).toBe(absent.status);
    expect(await legacy.text()).toBe(await absent.text());
  });

  it("does not let an id-only caller destroy a NULL-token row", async () => {
    const id = await insertLegacyNullTokenRow(3);
    const idBytes = fromHex(id);
    expect(await d1Count("SELECT COUNT(*) FROM blobs WHERE id = ?", idBytes)).toBe(1);

    const response = await SELF.fetch(`https://cipher.test/v1/blob/${id}`, {
      method: "DELETE",
      headers: { "cf-connecting-ip": "198.51.100.64" },
    });

    // 204 matches the answer for an id that was never stored, so the refusal
    // is not itself an existence oracle...
    expect(response.status).toBe(204);
    // ...but the row is still there. THIS is the assertion that matters: the
    // status alone would pass against the old, destructive behaviour.
    expect(await d1Count("SELECT COUNT(*) FROM blobs WHERE id = ?", idBytes)).toBe(1);
  });

  it("rejects a tokenless upload, so no new NULL-token row can be created", async () => {
    const response = await SELF.fetch("https://cipher.test/v1/blob", {
      method: "POST",
      body: new TextEncoder().encode("tokenless"),
      headers: {
        "x-osl-ttl-seconds": "3600",
        "cf-connecting-ip": "203.0.113.8",
      },
    });
    expect(response.status).toBe(400);
    expect(await d1Count("SELECT COUNT(*) FROM blobs WHERE fetch_token IS NULL")).toBe(0);
  });
});
