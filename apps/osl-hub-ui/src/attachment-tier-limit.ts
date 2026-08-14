export type AttachmentTierAccess = "free" | "pro" | "offlineGrace";

const MEBIBYTE = 1024 * 1024;

export type AttachmentTierLimit = {
  tierLabel: "Free" | "Pro";
  perFileLimitLabel: "25 MB" | "1 GB";
  maxBytes: number;
};

export type AttachmentTierRefusal = {
  filename: string;
  exactBytes: number;
  displayedSize: string;
  limit: AttachmentTierLimit;
  upgradeOffer: boolean;
};

export function attachmentTierLimit(access: AttachmentTierAccess): AttachmentTierLimit {
  return access === "free"
    ? { tierLabel: "Free", perFileLimitLabel: "25 MB", maxBytes: 25 * MEBIBYTE }
    : { tierLabel: "Pro", perFileLimitLabel: "1 GB", maxBytes: 1024 * MEBIBYTE };
}

/** Keeps the selected size and exact byte count together in the refusal. */
export function attachmentTierRefusal(access: AttachmentTierAccess, filename: string, exactBytes: number): AttachmentTierRefusal | null {
  const limit = attachmentTierLimit(access);
  if (!Number.isSafeInteger(exactBytes) || exactBytes <= limit.maxBytes) return null;
  const wholeMegabytes = exactBytes / MEBIBYTE;
  const displayedSize = Number.isInteger(wholeMegabytes) ? `${wholeMegabytes} MB` : `${wholeMegabytes.toFixed(1)} MB`;
  return { filename, exactBytes, displayedSize, limit, upgradeOffer: access === "free" };
}
