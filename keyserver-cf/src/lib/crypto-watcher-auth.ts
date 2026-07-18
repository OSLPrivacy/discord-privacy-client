const MAX_CLOCK_SKEW_SECONDS = 300;
const REQUEST_DOMAIN = "osl-watcher-request-v1";
const SETTLEMENT_DOMAIN = "osl-crypto-settlement-v1";

export interface WatcherSettlementEvidence {
  event_id: string;
  invoice_id: string;
  payment_method: "btc" | "xmr";
  amount_atomic: string;
  confirmations: number;
  observed_at: number;
  payment_reference_commitment: string;
}

function bytesToHex(bytes: Uint8Array): string {
  let result = "";
  for (const byte of bytes) result += byte.toString(16).padStart(2, "0");
  return result;
}

async function hmacHex(secret: string, value: string): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signature = await crypto.subtle.sign(
    "HMAC",
    key,
    new TextEncoder().encode(value),
  );
  return bytesToHex(new Uint8Array(signature));
}

function requestCanonical(
  method: string,
  path: string,
  timestamp: string,
  bodyHash: string,
): string {
  return [REQUEST_DOMAIN, method, path, timestamp, bodyHash].join("\n");
}

export function settlementCanonical(
  method: string,
  path: string,
  timestamp: string,
  evidence: WatcherSettlementEvidence,
): string {
  return [
    SETTLEMENT_DOMAIN,
    method,
    path,
    timestamp,
    evidence.event_id,
    evidence.invoice_id,
    evidence.payment_method,
    evidence.amount_atomic,
    String(evidence.confirmations),
    String(evidence.observed_at),
    evidence.payment_reference_commitment,
  ].join("\n");
}

export async function signedWatcherRequestHeaders(
  secret: string,
  method: string,
  path: string,
  body: string,
  nowSeconds = Math.floor(Date.now() / 1000),
): Promise<Record<string, string>> {
  const timestamp = String(nowSeconds);
  const canonical = requestCanonical(method, path, timestamp, await sha256Hex(body));
  return {
    "content-type": "application/json",
    "x-osl-timestamp": timestamp,
    "x-osl-request-signature": await hmacHex(secret, canonical),
  };
}

function decodeBase64(value: string, expectedLength: number): Uint8Array | null {
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(value)) return null;
  try {
    const decoded = Uint8Array.from(atob(value), (character) => character.charCodeAt(0));
    return decoded.length === expectedLength ? decoded : null;
  } catch {
    return null;
  }
}

export async function verifyWatcherSettlementSignature(
  headers: Headers,
  publicKeyBase64: string,
  method: string,
  path: string,
  evidence: WatcherSettlementEvidence,
  nowSeconds = Math.floor(Date.now() / 1000),
): Promise<boolean> {
  const timestamp = headers.get("x-osl-timestamp");
  const signatureBase64 = headers.get("x-osl-settlement-signature");
  if (!timestamp || !signatureBase64 || !/^\d{10}$/.test(timestamp)) {
    return false;
  }
  const parsed = Number.parseInt(timestamp, 10);
  if (Math.abs(nowSeconds - parsed) > MAX_CLOCK_SKEW_SECONDS) return false;
  const publicKeyBytes = decodeBase64(publicKeyBase64, 44);
  const signature = decodeBase64(signatureBase64, 64);
  if (!publicKeyBytes || !signature) return false;
  try {
    const publicKey = await crypto.subtle.importKey(
      "spki",
      publicKeyBytes,
      { name: "Ed25519" },
      false,
      ["verify"],
    );
    return await crypto.subtle.verify(
      { name: "Ed25519" },
      publicKey,
      signature,
      new TextEncoder().encode(settlementCanonical(method, path, timestamp, evidence)),
    );
  } catch {
    return false;
  }
}

export async function sha256Hex(value: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return bytesToHex(new Uint8Array(digest));
}
