/// Blob identifiers are client-derived 160-bit pointers. The Worker must never
/// generate or reinterpret them: only the 32-hex transport spelling is valid.
export const BLOB_ID_HEX_LENGTH = 32;

export function isBlobId(value: string): boolean {
  return new RegExp(`^[0-9a-f]{${BLOB_ID_HEX_LENGTH}}$`, "i").test(value);
}
