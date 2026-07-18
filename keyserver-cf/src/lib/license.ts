/// License key generation, validation, and hashing.
///
/// Format: `OSL-XXXX-XXXX-XXXX-XXXX` (16 data chars + 3 inter-
/// group dashes + "OSL-" prefix = 23 chars total).
///
///   - First 14 chars: random, drawn from the full Crockford
///     Base32 alphabet `0123456789ABCDEFGHJKMNPQRSTVWXYZ` (32
///     chars; I/L/O/U omitted by Crockford by design).
///   - Last 2 chars: HMAC-SHA256 checksum over the 14-char body,
///     truncated to 10 bits and Crockford-Base32 encoded as 2 chars.
///
/// Entropy: 14 chars × 5 bits = 70 bits. 2^70 ≈ 1.2×10^21
/// possible bodies. Bearer-credential threat model — server
/// stores SHA-256(plaintext); plaintext is encrypted to a browser
/// delivery key before persistence and revealed once after payment.
///
/// Checksum: 10 bits of HMAC-SHA256(body, LICENSE_HMAC_SECRET)
/// catches all single-char typos and ~99.9% of two-char typos.
/// The secret is per-deployment so a leaked schema doesn't let
/// an attacker forge checksum-valid keys (still useless without
/// the body's 70 bits of entropy, but defence-in-depth).

const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const BODY_LEN = 14;
const CHECKSUM_LEN = 2;
const PRODUCTION_PREFIX = "OSL-";
const QA_PREFIX = "OSLQ-";

export type LicenseIssuer = "production" | "qa";

function prefixForIssuer(issuer: LicenseIssuer): string {
  return issuer === "qa" ? QA_PREFIX : PRODUCTION_PREFIX;
}

/** Encode `n` bits at byte offset 0 of `bytes` as Crockford
 *  Base32, packed big-endian. */
function encodeBase32(bytes: Uint8Array, charCount: number): string {
  let bits = 0;
  let value = 0;
  let out = "";
  for (let i = 0; i < bytes.length && out.length < charCount; i++) {
    value = (value << 8) | (bytes[i] ?? 0);
    bits += 8;
    while (bits >= 5 && out.length < charCount) {
      const idx = (value >> (bits - 5)) & 0x1f;
      out += CROCKFORD[idx];
      bits -= 5;
    }
  }
  while (out.length < charCount) {
    // Pad from the residual bits. Shouldn't trigger when called
    // with enough source bytes (we always have ≥ 32 bytes from SHA-256
    // or 16 bytes from the random body).
    const idx = (value << (5 - bits)) & 0x1f;
    out += CROCKFORD[idx];
    bits = 0;
  }
  return out;
}

/** Generate the random body half of a license. CSPRNG-backed. */
function generateRandomBody(): string {
  // 9 bytes = 72 bits, easily enough to fill 14 × 5 = 70 bits.
  const raw = new Uint8Array(9);
  crypto.getRandomValues(raw);
  return encodeBase32(raw, BODY_LEN);
}

async function computeChecksum(body: string, hmacSecret: string): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(hmacSecret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const sigBuf = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(body));
  return encodeBase32(new Uint8Array(sigBuf), CHECKSUM_LEN);
}

function formatLicense(
  body14: string,
  checksum2: string,
  issuer: LicenseIssuer = "production",
): string {
  const data = body14 + checksum2; // 16 chars
  return (
    prefixForIssuer(issuer) +
    data.slice(0, 4) +
    "-" +
    data.slice(4, 8) +
    "-" +
    data.slice(8, 12) +
    "-" +
    data.slice(12, 16)
  );
}

export interface LicenseGenerated {
  /** The full key in display form, e.g. `OSL-4Q2H-7VPA-3KZM-XYRB`. */
  plaintext: string;
  /** SHA-256 lowercase hex digest for D1 storage. */
  hash: string;
}

/** Generate a fresh license key. Returns both the one-time plaintext
 *  and its SHA-256 hash for D1. */
export async function generateLicenseKey(
  hmacSecret: string,
  issuer: LicenseIssuer = "production",
): Promise<LicenseGenerated> {
  const body = generateRandomBody();
  const checksum = await computeChecksum(body, hmacSecret);
  const plaintext = formatLicense(body, checksum, issuer);
  const hash = await hashLicense(plaintext);
  return { plaintext, hash };
}

/**
 * Deterministic issuance for an already-random opaque invoice id. Replaying a
 * trusted settlement callback therefore produces the same bearer credential,
 * so a retry can never mint a second entitlement. The HMAC secret remains the
 * only way to derive the 70-bit body; no payment or customer data is encoded.
 */
export async function generateInvoiceLicenseKey(
  hmacSecret: string,
  invoiceId: string,
  issuer: LicenseIssuer = "production",
): Promise<LicenseGenerated> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(hmacSecret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signature = await crypto.subtle.sign(
    "HMAC",
    key,
    new TextEncoder().encode(`crypto-invoice-license-v1\0${invoiceId}`),
  );
  const body = encodeBase32(new Uint8Array(signature), BODY_LEN);
  const checksum = await computeChecksum(body, hmacSecret);
  const plaintext = formatLicense(body, checksum, issuer);
  return { plaintext, hash: await hashLicense(plaintext) };
}

/** Compute SHA-256(plaintext) as lowercase hex. The PK of the
 *  `licenses` table — never the plaintext itself. */
export async function hashLicense(plaintext: string): Promise<string> {
  const buf = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(plaintext),
  );
  const bytes = new Uint8Array(buf);
  let hex = "";
  for (const b of bytes) hex += b.toString(16).padStart(2, "0");
  return hex;
}

/** Normalise user-entered license: Crockford `O→0`, `I/L→1`,
 *  case-insensitive, dashes/spaces removed. Returns the
 *  canonical `OSL-XXXX-XXXX-XXXX-XXXX` form OR `null` if the
 *  input doesn't decode to 16 data chars. */
export function normalizeLicense(
  input: string,
  issuer: LicenseIssuer = "production",
): string | null {
  let s = input.toUpperCase().replace(/[\s-]/g, "");
  const compactPrefix = issuer === "qa" ? "OSLQ" : "OSL";
  if (s.startsWith(compactPrefix)) s = s.slice(compactPrefix.length);
  // Crockford ambiguity collapse.
  s = s.replace(/O/g, "0").replace(/[IL]/g, "1");
  if (s.length !== 16) return null;
  for (const c of s) {
    if (CROCKFORD.indexOf(c) < 0) return null;
  }
  return formatLicense(s.slice(0, BODY_LEN), s.slice(BODY_LEN), issuer);
}

/** Verify a license's checksum against the body. Returns
 *  true iff the 2-char trailer matches what would be computed
 *  for the 14-char body under `hmacSecret`. Does NOT consult
 *  the DB — that's `lookupLicense`'s job. */
export async function validateChecksum(
  plaintext: string,
  hmacSecret: string,
  issuer: LicenseIssuer = "production",
): Promise<boolean> {
  const norm = normalizeLicense(plaintext, issuer);
  if (!norm) return false;
  // Strip prefix + dashes back to the 16 data chars.
  const data = norm.slice(prefixForIssuer(issuer).length).replace(/-/g, "");
  const body = data.slice(0, BODY_LEN);
  const claimedChecksum = data.slice(BODY_LEN);
  const expected = await computeChecksum(body, hmacSecret);
  // Length is fixed; constant-time char compare.
  if (claimedChecksum.length !== expected.length) return false;
  let diff = 0;
  for (let i = 0; i < expected.length; i++) {
    diff |= claimedChecksum.charCodeAt(i) ^ expected.charCodeAt(i);
  }
  return diff === 0;
}
