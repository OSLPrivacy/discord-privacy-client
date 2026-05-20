/// 8-byte random IDs. 64 bits is sufficient: even with 1M live
/// blobs the birthday-collision probability is ~3e-8, and the
/// server rejects an upload that would collide with an existing
/// row (D1 PRIMARY KEY constraint), so the client just retries.

const ID_BYTES = 8;

export function newBlobId(): Uint8Array {
  const id = new Uint8Array(ID_BYTES);
  crypto.getRandomValues(id);
  return id;
}

/// Hex encoding for URL routing. 8 bytes → 16 hex chars.
export function idToHex(id: Uint8Array): string {
  let hex = "";
  for (const b of id) hex += b.toString(16).padStart(2, "0");
  return hex;
}

export function hexToId(hex: string): Uint8Array | null {
  if (hex.length !== ID_BYTES * 2) return null;
  if (!/^[0-9a-fA-F]+$/.test(hex)) return null;
  const out = new Uint8Array(ID_BYTES);
  for (let i = 0; i < ID_BYTES; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}
