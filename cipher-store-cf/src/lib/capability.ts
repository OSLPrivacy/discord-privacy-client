/**
 * Pointer and delivery-tag derivation shared with the client.
 *
 * HKDF uses an empty salt.  The exact labels and truncation are pinned by
 * `test/fixtures/pointer-vectors.json`; do not change either independently.
 */
const encoder = new TextEncoder();
const EMPTY_SALT = new Uint8Array();
const POINTER_BYTES = 20;
const OUTPUT_BITS = 128;

export interface PointerDerivationInput {
  pointer: Uint8Array;
  messageKey: Uint8Array;
  sendKey: Uint8Array;
  conversationKey: Uint8Array;
}

export interface PointerCapabilities {
  blobId: string;
  fetchCap: string;
  ackCap: string;
  manageCap: string;
  deliveryTag: string;
}

function concat(...parts: Uint8Array[]): Uint8Array {
  const length = parts.reduce((total, part) => total + part.byteLength, 0);
  const result = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.byteLength;
  }
  return result;
}

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function hkdf128(keyMaterial: Uint8Array, info: Uint8Array): Promise<Uint8Array> {
  const key = await crypto.subtle.importKey("raw", keyMaterial, "HKDF", false, ["deriveBits"]);
  const output = await crypto.subtle.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt: EMPTY_SALT, info },
    key,
    OUTPUT_BITS,
  );
  return new Uint8Array(output);
}

/** Derives the complete server-facing pointer tuple and its delivery wakeup tag. */
export async function derivePointerCapabilities(
  input: PointerDerivationInput,
): Promise<PointerCapabilities> {
  if (input.pointer.byteLength !== POINTER_BYTES) {
    throw new RangeError("pointer must be exactly 20 bytes (160 bits)");
  }

  const blobId = await hkdf128(input.pointer, encoder.encode("osl/ptr/id/v1"));
  const blobIdHex = hex(blobId);
  const [fetchCap, ackCap, manageCap, deliveryTag] = await Promise.all([
    hkdf128(input.pointer, encoder.encode("osl/ptr/fetch/v1")),
    hkdf128(input.messageKey, concat(encoder.encode("osl/ptr/ack/v1"), blobId)),
    hkdf128(input.sendKey, concat(encoder.encode("osl/ptr/manage/v1"), blobId)),
    hkdf128(input.conversationKey, encoder.encode("osl/tag/v1")),
  ]);

  return {
    blobId: blobIdHex,
    fetchCap: hex(fetchCap),
    ackCap: hex(ackCap),
    manageCap: hex(manageCap),
    deliveryTag: hex(deliveryTag),
  };
}
