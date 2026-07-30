export interface ScrubScopeFingerprintInput {
  serviceId: string;
  accountId: string;
  scanScope: string;
  findingCategories: readonly string[];
}

const SCOPE_FINGERPRINT_DOMAIN = "OSL/scrub-scope-fingerprint/v1";
const MAX_FIELD_BYTES = 256;
const MAX_FINDING_CATEGORIES = 64;

export async function computeScopeFingerprint(input: ScrubScopeFingerprintInput): Promise<string> {
  const encoded = encodeScopeFingerprintInput(input);
  const data = new ArrayBuffer(encoded.byteLength);
  new Uint8Array(data).set(encoded);
  const digest = await globalThis.crypto.subtle.digest("SHA-256", data);
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

export function encodeScopeFingerprintInput(input: ScrubScopeFingerprintInput): Uint8Array {
  if (!validToken(input.serviceId)
    || !validToken(input.accountId)
    || !boundedText(input.scanScope)
    || !Array.isArray(input.findingCategories)
    || input.findingCategories.length < 1
    || input.findingCategories.length > MAX_FINDING_CATEGORIES
    || input.findingCategories.some((category) => !validToken(category))) {
    throw new Error("invalid scrub scope fingerprint input");
  }

  const categories = [...input.findingCategories].sort();
  if (new Set(categories).size !== categories.length) {
    throw new Error("invalid scrub scope fingerprint input");
  }

  const writer = new LengthPrefixedWriter();
  writer.field("domain", SCOPE_FINGERPRINT_DOMAIN);
  writer.field("serviceId", input.serviceId);
  writer.field("accountId", input.accountId);
  writer.field("scanScope", input.scanScope);
  writer.u32(categories.length);
  for (const category of categories) writer.field("findingCategory", category);
  return writer.finish();
}

class LengthPrefixedWriter {
  private readonly bytes: number[] = [];
  private readonly encoder = new TextEncoder();

  field(label: string, value: string): void {
    this.writeBytes(this.encoder.encode(label));
    this.writeBytes(this.encoder.encode(value));
  }

  u32(value: number): void {
    this.bytes.push((value >>> 24) & 0xff, (value >>> 16) & 0xff, (value >>> 8) & 0xff, value & 0xff);
  }

  finish(): Uint8Array {
    return new Uint8Array(this.bytes);
  }

  private writeBytes(value: Uint8Array): void {
    this.u32(value.byteLength);
    this.bytes.push(...value);
  }
}

function validToken(value: string): boolean {
  return boundedText(value) && /^[a-z0-9][a-z0-9_-]{0,63}$/u.test(value);
}

function boundedText(value: string): boolean {
  return typeof value === "string"
    && value.length > 0
    && new TextEncoder().encode(value).length <= MAX_FIELD_BYTES
    && !/[\u0000-\u001f\u007f]/u.test(value);
}
