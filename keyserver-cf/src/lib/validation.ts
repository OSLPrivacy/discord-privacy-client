/// Cheap shape checks ported from keyserver/src/server.js. These
/// validate *form*, not cryptographic content — the Rust client
/// is the trust boundary for key validity.

const BASE64_RE = /^[A-Za-z0-9+/]+={0,2}$/;

export function isNonEmptyBase64(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && BASE64_RE.test(value);
}

export function isPlainString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

export function isPositiveInt(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 1;
}

export function isNonNegativeInt(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0;
}

export function isIso8601(value: unknown): value is string {
  return typeof value === "string" && !Number.isNaN(Date.parse(value));
}

/** Decode a base64 string into Uint8Array. Throws on malformed input. */
export function decodeBase64(value: string): Uint8Array {
  const bin = atob(value);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
