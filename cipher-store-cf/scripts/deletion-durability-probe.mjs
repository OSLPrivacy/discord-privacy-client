#!/usr/bin/env node

// This deliberately reads R2 through Cloudflare's control plane, never through
// the Worker URL. A cached custom domain can therefore never turn a stale GET
// into false deletion evidence.
const CAPABILITY_BYTES = 16;
const PAYLOAD = new Uint8Array(16);

function fail(message) {
  throw new Error(message);
}

function hex(bytes) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function capability() {
  const bytes = new Uint8Array(CAPABILITY_BYTES);
  crypto.getRandomValues(bytes);
  return hex(bytes);
}

async function sha256Hex(value) {
  return hex(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value))));
}

function previewWorkerUrl(value) {
  const url = new URL(value);
  if (url.protocol !== "https:" || !url.hostname.endsWith(".workers.dev")) {
    fail("--worker must be an HTTPS workers.dev preview URL; cached custom domains are forbidden");
  }
  url.pathname = url.pathname.replace(/\/+$/, "");
  url.search = "";
  url.hash = "";
  return url.toString().replace(/\/$/, "");
}

export function parseArgs(argv) {
  const args = { worker: null, accountId: null, bucket: null, yes: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--yes") { args.yes = true; continue; }
    if (arg === "--worker") { args.worker = argv[++index] ?? null; continue; }
    if (arg === "--account-id") { args.accountId = argv[++index] ?? null; continue; }
    if (arg === "--bucket") { args.bucket = argv[++index] ?? null; continue; }
    fail(`unknown argument: ${arg}`);
  }
  if (!args.worker || !args.accountId || !args.bucket) {
    fail("--worker, --account-id, and --bucket are required");
  }
  args.worker = previewWorkerUrl(args.worker);
  return args;
}

function apiObjectUrl(apiBase, accountId, bucket, key) {
  return `${apiBase}/accounts/${encodeURIComponent(accountId)}/r2/buckets/${encodeURIComponent(bucket)}/objects/${encodeURIComponent(key)}`;
}

function apiBucketUrl(apiBase, accountId, bucket, suffix = "") {
  return `${apiBase}/accounts/${encodeURIComponent(accountId)}/r2/buckets/${encodeURIComponent(bucket)}${suffix}`;
}

async function apiJson(fetchImpl, url, token) {
  const response = await fetchImpl(url, { headers: { authorization: `Bearer ${token}` } });
  const body = await response.json().catch(() => null);
  if (!response.ok || !body?.success) fail(`Cloudflare API ${response.status} for ${new URL(url).pathname}`);
  return body.result;
}

export async function runProbe({ worker, accountId, bucket, token, fetchImpl = fetch, apiBase = "https://api.cloudflare.com/client/v4" }) {
  if (!token) fail("CLOUDFLARE_API_TOKEN is required");
  const fetchCap = capability();
  const ackCap = capability();
  const manageCap = capability();
  const blobId = capability();
  const [fetchDigest, ackDigest, manageDigest, deliveryTag] = await Promise.all([
    sha256Hex(fetchCap), sha256Hex(ackCap), sha256Hex(manageCap), sha256Hex(capability()),
  ]);
  let uploaded = false;
  try {
    const upload = await fetchImpl(`${worker}/v1/blob`, {
      method: "POST",
      headers: {
        "content-length": String(PAYLOAD.byteLength),
        "content-type": "application/octet-stream",
        "x-osl-ttl-seconds": "3600",
        "x-osl-blob-id": blobId,
        "x-osl-fetch-digest": fetchDigest,
        "x-osl-ack-digest": ackDigest,
        "x-osl-manage-digest": manageDigest,
        "x-osl-delivery-tag": deliveryTag.slice(0, 32),
        "x-osl-object-class": "single-ack",
      },
      body: PAYLOAD,
    });
    if (upload.status !== 201) fail(`upload returned HTTP ${upload.status}`);
    uploaded = true;

    const ack = await fetchImpl(`${worker}/v1/blob/${blobId}/ack`, {
      method: "POST",
      headers: { "x-osl-ack-cap": ackCap },
    });
    if (ack.status !== 204) fail(`ack returned HTTP ${ack.status}`);

    // This is an uncached HEAD against the exact R2 object addressed by the
    // PAYLOADS binding's key convention (SHA-256(fetch_cap)).
    const object = await fetchImpl(apiObjectUrl(apiBase, accountId, bucket, fetchDigest), {
      method: "HEAD",
      headers: { authorization: `Bearer ${token}` },
    });
    if (object.status !== 404) fail(`deleted payload is still observable through R2 (HTTP ${object.status})`);

    const [bucketInfo, locks] = await Promise.all([
      apiJson(fetchImpl, apiBucketUrl(apiBase, accountId, bucket), token),
      apiJson(fetchImpl, apiBucketUrl(apiBase, accountId, bucket, "/lock"), token),
    ]);
    if (bucketInfo?.name !== bucket) fail("Cloudflare API returned a different payload bucket");
    if (!Array.isArray(locks) || locks.length !== 0) fail("payload bucket has a lock rule; deletion is not durable");

    // R2 exposes no bucket-versioning configuration: its S3 compatibility
    // matrix explicitly marks GetBucketVersioning unsupported. Keep this fact
    // explicit in the receipt so a future platform change forces review.
    return { blobId, objectKey: fetchDigest, r2HeadStatus: object.status, lockRules: locks.length, versioning: "unsupported-by-r2" };
  } finally {
    if (uploaded) {
      await fetchImpl(`${worker}/v1/blob/${blobId}`, {
        method: "DELETE",
        headers: { "x-osl-manage-cap": manageCap },
      }).catch(() => undefined);
    }
  }
}

function usage() {
  console.log("Usage: CLOUDFLARE_API_TOKEN=… node scripts/deletion-durability-probe.mjs --worker https://<preview>.workers.dev --account-id <id> --bucket osl-cipher-payloads-prod [--yes]");
}

async function main() {
  let args;
  try { args = parseArgs(process.argv.slice(2)); } catch (error) { usage(); console.error(error.message); process.exitCode = 1; return; }
  if (!args.yes) {
    console.log("Dry run: no upload is sent. Re-run with --yes to probe the preview Worker and its PAYLOADS R2 bucket.");
    return;
  }
  try {
    const receipt = await runProbe({ ...args, token: process.env.CLOUDFLARE_API_TOKEN });
    console.log(`PASS deletion durability: R2 HEAD=${receipt.r2HeadStatus} locks=${receipt.lockRules} versioning=${receipt.versioning} key=${receipt.objectKey}`);
  } catch (error) {
    console.error(`FAIL deletion durability: ${error.message}`);
    process.exitCode = 1;
  }
}

if (process.argv[1] && new URL(`file://${process.argv[1]}`).href === import.meta.url) await main();
