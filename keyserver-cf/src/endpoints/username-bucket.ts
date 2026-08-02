import type { Env } from "../env.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { badRequest, tooMany } from "../lib/http.js";

const ROWS = 1024;
const PREFIX_RE = /^[0-9a-f]{4}$/;
const DOMAIN = "OSL-USERNAME-BUCKET-v1";
const encoder = new TextEncoder();

type DirectoryRow = { username: string; user_id: string; ik_ed25519_pub: string };
type BucketRow = { suffix: string; userId: string; ed25519: string };

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function b64(bytes: Uint8Array): string {
  let value = "";
  for (const byte of bytes) value += String.fromCharCode(byte);
  return btoa(value);
}

async function digestUsername(username: string): Promise<string> {
  return hex(new Uint8Array(await crypto.subtle.digest("SHA-256", encoder.encode(DOMAIN + username))));
}

async function decoy(secret: string, prefix: string, index: number): Promise<BucketRow> {
  const key = await crypto.subtle.importKey(
    "raw", encoder.encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"],
  );
  const material = new Uint8Array(await crypto.subtle.sign("HMAC", key, encoder.encode(`${prefix}${index}`)));
  // A fixed-width opaque id and 32-byte Ed25519-shaped value make decoys
  // indistinguishable by row length from protocol identities.
  return { suffix: hex(material).slice(4, 32), userId: `osl_${hex(material).slice(0, 52)}`, ed25519: b64(material) };
}

export async function handleUsernameBucket(request: Request, env: Env, prefix: string): Promise<Response> {
  if (!PREFIX_RE.test(prefix)) return badRequest("bucket prefix must be four lowercase hex characters");
  const limited = await checkRateLimit(env, callerIp(request), 120, "username-bucket-ip");
  if (!limited.ok) return tooMany(limited.retryAfter);
  if (!env.USERNAME_BUCKET_DECOY_SECRET) return new Response("bucket lookup unavailable", { status: 503 });

  const candidates = await env.DB.prepare(
    `SELECT d.username, d.user_id, u.ik_ed25519_pub
       FROM username_directory d JOIN users u ON u.user_id = d.user_id`,
  ).all<DirectoryRow>();
  const real: BucketRow[] = [];
  for (const row of candidates.results) {
    const key = await digestUsername(row.username);
    if (key.startsWith(prefix)) real.push({ suffix: key.slice(4, 32), userId: row.user_id, ed25519: row.ik_ed25519_pub });
  }
  // An overflow must not emit a short/partial bucket; operators must expand
  // the prefix width before admitting more than this fixed privacy floor.
  if (real.length > ROWS) return new Response("bucket capacity exceeded", { status: 503 });
  const occupied = new Set(real.map((row) => row.suffix));
  for (let index = 0; real.length < ROWS; index++) {
    const row = await decoy(env.USERNAME_BUCKET_DECOY_SECRET, prefix, index);
    if (!occupied.has(row.suffix)) { occupied.add(row.suffix); real.push(row); }
  }
  real.sort((a, b) => a.suffix.localeCompare(b.suffix));
  return new Response(real.map((row) => `${row.suffix}:${row.userId}:${row.ed25519}\n`).join(""), {
    headers: { "content-type": "application/octet-stream", "cache-control": "no-store", "x-content-type-options": "nosniff" },
  });
}
