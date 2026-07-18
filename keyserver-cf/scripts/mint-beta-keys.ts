/**
 * Owner comp batch client.
 *
 * The production HMAC root never leaves the Worker. This tool generates an
 * ephemeral RSA delivery key, calls the dual-authorized operator endpoint,
 * decrypts exactly one response locally, and writes the codes to a mode-0600
 * file. It never prints activation codes or emits SQL.
 */
import { chmodSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { webcrypto } from "node:crypto";

const subtle = webcrypto.subtle;
const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = join(SCRIPT_DIR, "out");
const AAD = new TextEncoder().encode("osl-comp-delivery-v1");

function base64(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("base64");
}

function base64url(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("base64url");
}

function parseArgs(argv: string[]) {
  const args = { count: 0, purpose: "", days: 0, url: "", out: "", revoke: "" };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--count") args.count = Number(argv[++index]);
    else if (arg === "--purpose") args.purpose = String(argv[++index] ?? "");
    else if (arg === "--days") args.days = Number(argv[++index]);
    else if (arg === "--url") args.url = String(argv[++index] ?? "");
    else if (arg === "--out") args.out = String(argv[++index] ?? "");
    else if (arg === "--revoke") args.revoke = String(argv[++index] ?? "");
    else throw new Error(`unknown argument: ${arg}`);
  }
  return args;
}

function authorizationHeaders(): Record<string, string> {
  const primary = process.env.OSL_KEYSERVER_ADMIN_TOKEN;
  const comp = process.env.OSL_COMP_ADMIN_TOKEN;
  if (!primary || !comp) {
    throw new Error("OSL_KEYSERVER_ADMIN_TOKEN and OSL_COMP_ADMIN_TOKEN must both be set");
  }
  return {
    authorization: `Bearer ${primary}`,
    "x-osl-comp-authorization": `Bearer ${comp}`,
  };
}

export interface CompDeliveryPayload {
  version: 1;
  batch_id: string;
  issuer: "production" | "qa";
  expires_at: number;
  activation_codes: string[];
}

export function parseDeliveryPayload(
  bytes: Uint8Array,
  expected: { batchId: string; quantity: number; expiresAt: number },
): CompDeliveryPayload {
  let parsed: unknown;
  try {
    parsed = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } finally {
    bytes.fill(0);
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("encrypted delivery payload is malformed");
  }
  const record = parsed as Record<string, unknown>;
  const exact = ["activation_codes", "batch_id", "expires_at", "issuer", "version"];
  if (Object.keys(record).sort().join("\0") !== exact.join("\0")) {
    throw new Error("encrypted delivery payload fields are unexpected");
  }
  if (
    record.version !== 1 ||
    record.batch_id !== expected.batchId ||
    record.expires_at !== expected.expiresAt ||
    (record.issuer !== "production" && record.issuer !== "qa") ||
    !Array.isArray(record.activation_codes) ||
    record.activation_codes.length !== expected.quantity
  ) {
    throw new Error("encrypted delivery payload does not match the issued batch");
  }
  const prefix = record.issuer === "qa" ? "OSLQ" : "OSL";
  const pattern = new RegExp(`^${prefix}-[0-9A-HJKMNP-TV-Z]{4}(?:-[0-9A-HJKMNP-TV-Z]{4}){3}$`);
  if (
    !record.activation_codes.every((code) => typeof code === "string" && pattern.test(code)) ||
    new Set(record.activation_codes).size !== record.activation_codes.length
  ) {
    throw new Error("encrypted delivery contains an invalid activation code");
  }
  return record as unknown as CompDeliveryPayload;
}

export function writeCompTextFile(outputPath: string, payload: CompDeliveryPayload): void {
  if (!outputPath.toLowerCase().endsWith(".txt")) {
    throw new Error("comp output path must end in .txt");
  }
  const text = [
    `OSL comp batch: ${payload.batch_id}`,
    `Issuer: ${payload.issuer}`,
    `Expires: ${new Date(payload.expires_at * 1000).toISOString()}`,
    "",
    ...payload.activation_codes,
    "",
  ].join("\n");
  const outputBytes = Buffer.from(text, "utf8");
  try {
    writeFileSync(outputPath, outputBytes, { mode: 0o600, flag: "wx" });
    // Some filesystems and compatibility layers apply the process umask or
    // ignore the create-mode hint. Reassert owner-only access after the
    // exclusive create so the promised secret-file boundary is explicit.
    chmodSync(outputPath, 0o600);
  } finally {
    outputBytes.fill(0);
    payload.activation_codes.fill("");
  }
}

async function revoke(url: string, batchId: string): Promise<void> {
  if (!/^comp_[0-9a-f]{32}$/.test(batchId)) throw new Error("invalid batch id");
  const response = await fetch(`${url}/v1/internal/comp/batches/${batchId}`, {
    method: "DELETE",
    headers: authorizationHeaders(),
  });
  if (!response.ok) throw new Error(`revocation failed (${response.status})`);
  console.log(`Comp batch ${batchId} revoked.`);
}

async function issue(args: ReturnType<typeof parseArgs>): Promise<void> {
  if (!Number.isInteger(args.count) || args.count < 1 || args.count > 25) {
    throw new Error("--count must be an integer from 1 to 25");
  }
  if (!Number.isInteger(args.days) || args.days < 1 || args.days > 366) {
    throw new Error("--days must be an integer from 1 to 366; permanent grants are forbidden");
  }
  if (args.purpose.length < 3 || args.purpose.length > 200) {
    throw new Error("--purpose must contain 3 to 200 characters");
  }

  const pair = await subtle.generateKey(
    {
      name: "RSA-OAEP",
      modulusLength: 2048,
      publicExponent: new Uint8Array([1, 0, 1]),
      hash: "SHA-256",
    },
    true,
    ["encrypt", "decrypt"],
  ) as CryptoKeyPair;
  const publicSpki = new Uint8Array(await subtle.exportKey("spki", pair.publicKey));
  const requestId = webcrypto.getRandomValues(new Uint8Array(32));
  const response = await fetch(`${args.url}/v1/internal/comp/batches`, {
    method: "POST",
    headers: { ...authorizationHeaders(), "content-type": "application/json" },
    body: JSON.stringify({
      quantity: args.count,
      purpose: args.purpose,
      expires_at: Math.floor(Date.now() / 1000) + args.days * 86400,
      request_id: base64url(requestId),
      delivery_public_key_spki: base64(publicSpki),
    }),
  });
  const result = await response.json() as {
    error?: string;
    batch_id?: string;
    audit_digest?: string;
    quantity?: number;
    expires_at?: number;
    delivery?: {
      algorithm?: string;
      wrapped_key?: string;
      nonce?: string;
      ciphertext?: string;
    };
  };
  if (
    !response.ok ||
    !result.batch_id ||
    result.quantity !== args.count ||
    !Number.isSafeInteger(result.expires_at) ||
    result.delivery?.algorithm !== "rsa-oaep-sha256+aes-256-gcm"
  ) {
    throw new Error(result.error || `issuance failed (${response.status})`);
  }

  const rawAes = await subtle.decrypt(
    { name: "RSA-OAEP" },
    pair.privateKey,
    Buffer.from(result.delivery.wrapped_key!, "base64"),
  );
  const aes = await subtle.importKey("raw", rawAes, "AES-GCM", false, ["decrypt"]);
  new Uint8Array(rawAes).fill(0);
  const plaintext = await subtle.decrypt(
    {
      name: "AES-GCM",
      iv: Buffer.from(result.delivery.nonce!, "base64"),
      additionalData: AAD,
    },
    aes,
    Buffer.from(result.delivery.ciphertext!, "base64"),
  );

  const payload = parseDeliveryPayload(new Uint8Array(plaintext), {
    batchId: result.batch_id,
    quantity: result.quantity,
    expiresAt: result.expires_at!,
  });
  mkdirSync(OUT_DIR, { recursive: true, mode: 0o700 });
  const outputPath = args.out || join(OUT_DIR, `${result.batch_id}.txt`);
  writeCompTextFile(outputPath, payload);
  console.log(`Comp batch ${result.batch_id} written once to ${outputPath} (mode 0600).`);
  console.log(`Audit digest: ${result.audit_digest}`);
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  if (!/^https:\/\//.test(args.url)) throw new Error("--url must be an HTTPS keyserver base URL");
  if (args.revoke) await revoke(args.url.replace(/\/$/, ""), args.revoke);
  else await issue({ ...args, url: args.url.replace(/\/$/, "") });
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : "comp operation failed");
    process.exitCode = 1;
  });
}
