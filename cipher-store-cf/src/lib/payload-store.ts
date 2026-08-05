import { sha256Hex } from "./digest.js";

/**
 * Byte-store portability seam for message payloads.
 *
 * The fetch capability is never used as an object name directly: the bucket
 * receives only its SHA-256 digest, which is also what the D1 index stores.
 */
export interface PayloadStore {
  put(fetchCap: string, bytes: Uint8Array): Promise<void>;
  get(fetchCap: string): Promise<Uint8Array | null>;
  delete(fetchCap: string): Promise<void>;
  /**
   * Write `bytes` under `fetchDigest` ONLY IF no object is stored there.
   *
   * D-264. This is the one write on this Worker whose object key is named by
   * the caller: `fetchDigest` is the `x-osl-fetch-digest` request header. The
   * attachment direct-upload path already conditions its put on
   * `onlyIf: { etagDoesNotMatch: "*" }` even though *it* names its own key
   * (`attachments/<server random>`), so the unconditional write was the strict
   * outlier — the weaker precondition sat on the strictly more exposed key.
   *
   * "The caller already had to know the digest" is not a sufficient reason to
   * allow the overwrite. It is the same reason the attachment path declined to
   * accept for itself, and it argues about who can *reach* the write rather
   * than what the write may *destroy*: reaching it needs a fetch capability,
   * while overwriting destroys a payload belonging to a row this caller does
   * not own and cannot delete.
   *
   * An existing object is left exactly as it was. The key is
   * SHA-256(fetch capability), so an object already stored under it is already
   * the payload for that capability: the only two ways to reach this branch are
   * re-presenting a capability whose bytes are therefore the caller's own, and
   * naming someone else's — which must not clobber.
   */
  putByDigest(fetchDigest: string, bytes: Uint8Array): Promise<void>;
  deleteByDigest(fetchDigest: string): Promise<void>;
}

/** Cloudflare R2 implementation of the payload-byte store. */
export class R2PayloadStore implements PayloadStore {
  constructor(private readonly bucket: R2Bucket) {}

  async put(fetchCap: string, bytes: Uint8Array): Promise<void> {
    await this.bucket.put(await this.objectKey(fetchCap), bytes);
  }

  async get(fetchCap: string): Promise<Uint8Array | null> {
    const object = await this.bucket.get(await this.objectKey(fetchCap));
    if (object === null) return null;
    return new Uint8Array(await object.arrayBuffer());
  }

  async delete(fetchCap: string): Promise<void> {
    await this.bucket.delete(await this.objectKey(fetchCap));
  }

  async putByDigest(fetchDigest: string, bytes: Uint8Array): Promise<void> {
    // A null result is R2 reporting the precondition failed, not an error. The
    // interface doc above says why that outcome is silent rather than raised:
    // reporting it would be a new answer about an object the caller was never
    // told about, which is D-255 on a different key space.
    await this.bucket.put(fetchDigest, bytes, {
      onlyIf: { etagDoesNotMatch: "*" },
    });
  }

  async deleteByDigest(fetchDigest: string): Promise<void> {
    await this.bucket.delete(fetchDigest);
  }

  private objectKey(fetchCap: string): Promise<string> {
    return sha256Hex(fetchCap);
  }
}
