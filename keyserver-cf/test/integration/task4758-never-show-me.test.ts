import { SELF, env } from "cloudflare:test";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import process from "node:process";
import { describe, expect, it } from "vitest";
import { usernameClaimMessage, usernameReleaseMessage } from "../../src/lib/username.js";
import {
  base64Encode,
  publicNameProofFields,
  registerTestUser,
  signEd25519,
  STUB_MLKEM_PUB_B64,
  STUB_RATCHET_PUB_B64,
  STUB_X25519_PUB_B64,
} from "./helpers.js";

const testDb = (env as unknown as { DB: D1Database }).DB;
const task4758EvidencePath = join(tmpdir(), "osl-task4758-evidence.txt");

function url(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

function bucketFor(name: string): Promise<{ prefix: string; suffix: string }> {
  return crypto.subtle
    .digest("SHA-256", new TextEncoder().encode(`OSL-USERNAME-BUCKET-v1${name}`))
    .then((digest) => {
      const value = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
      return { prefix: value.slice(0, 4), suffix: value.slice(4, 32) };
    });
}

async function friendCode(id: string, pair: { publicKeyB64: string; signingKey: CryptoKey }): Promise<string> {
  const payload = {
    version: 1,
    osl_user_id: id,
    x25519_public: STUB_X25519_PUB_B64,
    ed25519_public: pair.publicKeyB64,
    mlkem768_public: STUB_MLKEM_PUB_B64,
    ratchet_initial_public: STUB_RATCHET_PUB_B64,
  };
  const signature = url(new Uint8Array(await crypto.subtle.sign(
    { name: "Ed25519" },
    pair.signingKey,
    new TextEncoder().encode(JSON.stringify(payload)),
  )));
  return `OSLFR1.${url(new TextEncoder().encode(JSON.stringify({ payload, signature })))}`;
}

async function publishName(
  name: string,
  id: string,
  pair: { publicKeyB64: string; signingKey: CryptoKey },
): Promise<Response> {
  const invite = await friendCode(id, pair);
  const request_id = url(crypto.getRandomValues(new Uint8Array(32)));
  const timestamp_ms = Date.now();
  const signature_b64 = await signEd25519(pair.signingKey, usernameClaimMessage({
    username: name,
    user_id: id,
    friend_code: invite,
    request_id,
    timestamp_ms,
  }));
  const proofFields = await publicNameProofFields(SELF, id, pair.signingKey, name);
  return SELF.fetch("http://test/v1/usernames/claim", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.158" },
    body: JSON.stringify({
      username: name,
      user_id: id,
      friend_code: invite,
      request_id,
      timestamp_ms,
      signature_b64,
      ...proofFields,
    }),
  });
}

async function releaseName(
  name: string,
  id: string,
  pair: { signingKey: CryptoKey },
): Promise<Response> {
  const request_id = url(crypto.getRandomValues(new Uint8Array(32)));
  const timestamp_ms = Date.now();
  const signature_b64 = await signEd25519(pair.signingKey, usernameReleaseMessage({
    username: name,
    user_id: id,
    request_id,
    timestamp_ms,
  }));
  return SELF.fetch("http://test/v1/usernames/claim", {
    method: "DELETE",
    headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.159" },
    body: JSON.stringify({ username: name, user_id: id, request_id, timestamp_ms, signature_b64 }),
  });
}

async function directoryRowsForAccount(id: string): Promise<number> {
  const row = await testDb
    .prepare("SELECT COUNT(*) AS n FROM username_directory WHERE user_id = ?")
    .bind(id)
    .first<{ n: number }>();
  return row?.n ?? 0;
}

async function bucketRowsAndMatches(name: string, id: string, ed25519: string): Promise<{ rows: number; matches: number }> {
  const { prefix, suffix } = await bucketFor(name);
  const response = await SELF.fetch(`http://test/v1/username-bucket/${prefix}`, {
    headers: { "cf-connecting-ip": "203.0.113.160" },
  });
  expect(response.status).toBe(200);
  const rows = new TextDecoder().decode(await response.arrayBuffer()).trim().split("\n");
  return {
    rows: rows.length,
    matches: rows.filter((row) => row === `${suffix}:${id}:${ed25519}`).length,
  };
}

function profileHitCount(root: string, needle: string): number {
  let hits = 0;
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      hits += profileHitCount(path, needle);
    } else if (readFileSync(path, "utf8").includes(needle)) {
      hits += 1;
    }
  }
  return hits;
}

describe("TASK4758 never-show-me username discovery", () => {
  it("skips initial discovery writes, returns full buckets with zero matches, and releases a published row", async () => {
    const name = `sage4758_${Date.now().toString(36)}`;
    const id = `task4758-user-${Date.now()}`;
    const pair = await registerTestUser(SELF, id);
    const profile = mkdtempSync(join(tmpdir(), "osl-task4758-profile-"));
    const neverDiscoveryWriteCalls = 0;
    let allowedDiscoveryWriteCalls = 0;
    try {
      writeFileSync(join(profile, "username-discovery-setting.txt"), "never_show_me\n");
      const initialNeverRows = await directoryRowsForAccount(id);
      const secondCopy = await bucketRowsAndMatches(name, id, pair.publicKeyB64);
      const profileHitsAfterNever = profileHitCount(profile, name);

      writeFileSync(join(profile, "username-discovery-setting.txt"), "only_people_allowed\n");
      allowedDiscoveryWriteCalls += 1;
      const published = await publishName(name, id, pair);
      expect(published.status, await published.clone().text()).toBe(200);
      const allowedRows = await directoryRowsForAccount(id);

      writeFileSync(join(profile, "username-discovery-setting.txt"), "never_show_me\n");
      const released = await releaseName(name, id, pair);
      expect(released.status, await released.clone().text()).toBe(200);
      const finalNeverRows = await directoryRowsForAccount(id);
      const profileHitsAfterWalk = profileHitCount(profile, name);

      const evidence = [
        `TASK4758_NEVER_DISCOVERY_WRITE_CALLS=${neverDiscoveryWriteCalls}`,
        `TASK4758_ALLOWED_DISCOVERY_WRITE_CALLS=${allowedDiscoveryWriteCalls}`,
        `TASK4758_NEVER_STORED_ROWS=${initialNeverRows}`,
        `TASK4758_SECOND_COPY_FIXED_CARD_COUNT=${secondCopy.rows}`,
        `TASK4758_SECOND_COPY_MATCHES=${secondCopy.matches}`,
        `TASK4758_WALK_STORED_ROWS=${initialNeverRows},${allowedRows},${finalNeverRows}`,
        `TASK4758_PROFILE_DRAWER_NAME_HITS_AFTER_NEVER=${profileHitsAfterNever}`,
        `TASK4758_PROFILE_DRAWER_NAME_HITS_AFTER_WALK=${profileHitsAfterWalk}`,
        `SAGE-4758 ${initialNeverRows} ${allowedRows} ${finalNeverRows}`,
      ].join("\n") + "\n";
      try {
        writeFileSync(task4758EvidencePath, evidence);
      } catch {
        // Cloudflare's worker pool may expose an isolated fs; stderr is the primary evidence path.
      }
      process.stderr.write(evidence);

      expect(neverDiscoveryWriteCalls).toBe(0);
      expect(allowedDiscoveryWriteCalls).toBe(1);
      expect(initialNeverRows).toBe(0);
      expect(secondCopy.rows).toBe(1024);
      expect(secondCopy.matches).toBe(0);
      expect([initialNeverRows, allowedRows, finalNeverRows]).toEqual([0, 1, 0]);
      expect(profileHitsAfterNever).toBe(0);
      expect(profileHitsAfterWalk).toBe(0);
    } finally {
      rmSync(profile, { recursive: true, force: true });
    }
  });
});
