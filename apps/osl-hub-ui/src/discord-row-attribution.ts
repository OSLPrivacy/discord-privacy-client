export type NativeDiscordRowOrientation = "incoming" | "outgoing";
export type NativeDiscordRowPoster = "self_account" | "peer_account";

/**
 * Backend-produced correlation proof for one decrypted Discord history row.
 *
 * This is response-only data. The renderer never supplies any of these fields
 * to the native command and uses them only after strict parsing.
 */
export interface NativeDiscordRowAttribution {
  discordMessageId: string;
  posterIdentitySha256: string;
  poster: NativeDiscordRowPoster;
  nativeLocatorSha256: string;
  carrierSha256: string;
  blobId: string;
  ciphertextSha256: string;
  payloadId: string;
  scopeBindingSha256: string;
  windowGeneration: number;
  orientation: NativeDiscordRowOrientation;
}

const ATTRIBUTION_KEYS = [
  "discordMessageId",
  "posterIdentitySha256",
  "poster",
  "nativeLocatorSha256",
  "carrierSha256",
  "blobId",
  "ciphertextSha256",
  "payloadId",
  "scopeBindingSha256",
  "windowGeneration",
  "orientation",
] as const;
const SHA256 = /^[0-9a-f]{64}$/u;
const BLOB_ID = /^[0-9a-f]{16}$/u;
const encoder = new TextEncoder();

function exactKeys(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && expected.every((key, index) => key === actual[index]);
}

function boundedId(value: unknown): value is string {
  return typeof value === "string"
    && value.length > 0
    && encoder.encode(value).byteLength <= 96
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

export function parseNativeDiscordRowAttribution(
  value: unknown,
): NativeDiscordRowAttribution | null {
  if (!exactKeys(value, ATTRIBUTION_KEYS)) return null;
  const record = value as Record<string, unknown>;
  if (!boundedId(record.discordMessageId)
    || !boundedId(record.payloadId)
    || typeof record.posterIdentitySha256 !== "string"
    || !SHA256.test(record.posterIdentitySha256)
    || typeof record.nativeLocatorSha256 !== "string"
    || !SHA256.test(record.nativeLocatorSha256)
    || typeof record.carrierSha256 !== "string"
    || !SHA256.test(record.carrierSha256)
    || typeof record.blobId !== "string"
    || !BLOB_ID.test(record.blobId)
    || typeof record.ciphertextSha256 !== "string"
    || !SHA256.test(record.ciphertextSha256)
    || typeof record.scopeBindingSha256 !== "string"
    || !SHA256.test(record.scopeBindingSha256)
    || !Number.isSafeInteger(record.windowGeneration)
    || Number(record.windowGeneration) <= 0) return null;
  const orientation = record.orientation;
  const poster = record.poster;
  if ((poster !== "self_account" || orientation !== "outgoing")
    && (poster !== "peer_account" || orientation !== "incoming")) return null;
  return record as unknown as NativeDiscordRowAttribution;
}

/** Refuse an ambiguous response even if every proof is individually well formed. */
export function nativeDiscordAttributionsAreUnique(
  attributions: readonly NativeDiscordRowAttribution[],
): boolean {
  const fields: Array<keyof NativeDiscordRowAttribution> = [
    "discordMessageId",
    "nativeLocatorSha256",
    "carrierSha256",
    "blobId",
    "ciphertextSha256",
    "payloadId",
  ];
  return fields.every((field) => {
    const values = attributions.map((proof) => proof[field]);
    return new Set(values).size === values.length;
  });
}
