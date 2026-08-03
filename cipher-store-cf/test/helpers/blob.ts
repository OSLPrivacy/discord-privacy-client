import { sha256Hex } from "../../src/lib/digest.js";

export const DEFAULT_BLOB_BYTES = new Uint8Array([7]);

export interface BlobCapabilities {
  id: string;
  fetchCap: string;
  ackCap: string;
  manageCap: string;
  deliveryTag: string;
}

export function blobCapabilities(id: string): BlobCapabilities {
  return {
    id,
    fetchCap: `2${id.slice(1)}`,
    ackCap: `3${id.slice(1)}`,
    manageCap: `4${id.slice(1)}`,
    deliveryTag: `5${id.slice(1)}`,
  };
}

export async function blobUploadHeaders(
  capabilities: BlobCapabilities,
  objectClass: "single-ack" | "multi-fetch" = "single-ack",
  ttl = "3600",
): Promise<Record<string, string>> {
  return {
    "x-osl-ttl-seconds": ttl,
    ...(ttl === "604800" ? {} : { "x-osl-expiry-mode": "absolute" }),
    "x-osl-blob-id": capabilities.id,
    "x-osl-fetch-digest": await sha256Hex(capabilities.fetchCap),
    "x-osl-ack-digest": await sha256Hex(capabilities.ackCap),
    "x-osl-manage-digest": await sha256Hex(capabilities.manageCap),
    "x-osl-object-class": objectClass,
    "x-osl-delivery-tag": capabilities.deliveryTag,
  };
}

export function blobFetchHeaders(fetchCap: string): Record<string, string> {
  return { "x-osl-fetch-cap": fetchCap };
}
