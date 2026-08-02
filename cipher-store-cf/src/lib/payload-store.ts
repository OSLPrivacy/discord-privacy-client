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
    await this.bucket.put(fetchDigest, bytes);
  }

  async deleteByDigest(fetchDigest: string): Promise<void> {
    await this.bucket.delete(fetchDigest);
  }

  private objectKey(fetchCap: string): Promise<string> {
    return sha256Hex(fetchCap);
  }
}
