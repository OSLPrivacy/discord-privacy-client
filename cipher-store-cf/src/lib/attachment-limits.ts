export type AttachmentTier = "free" | "pro";

export interface AttachmentTierLimit {
  readonly label: "Free" | "Pro";
  readonly tier: "Free" | "Pro";
  /** Product plaintext limit. */
  readonly maxBytesPerFile: number;
  readonly max_file_bytes: number;
  /** Opaque post-AEAD ceiling accepted by the store. */
  readonly maxSealedBytesPerFile: number;
  readonly maxFilesPerMessage: number;
  readonly max_files_per_message: number;
  readonly perFileLabel: string;
}

export interface AttachmentLimitCheck {
  readonly accepted: boolean;
  readonly tier: AttachmentTier;
  readonly tierLabel: string;
  readonly sizeBytes: number;
  readonly fileCount: number;
  readonly maxBytesPerFile: number;
  readonly maxFilesPerMessage: number;
  readonly reason: "accepted" | "file_too_large" | "too_many_files";
  readonly firstRejectedFile: number | null;
}

const MIB = 1024 * 1024;
const GIB = 1024 * MIB;

export const ATTACHMENT_TIER_LIMITS: Record<AttachmentTier, AttachmentTierLimit> = {
  free: {
    label: "Free",
    tier: "Free",
    maxBytesPerFile: 25 * MIB,
    max_file_bytes: 25 * MIB,
    // A 25 MiB plaintext plus its authenticated length prefix occupies the
    // next 50 MiB padding bucket. Allow bounded header/tag overhead too.
    maxSealedBytesPerFile: 50 * MIB + 64 * 1024,
    maxFilesPerMessage: 16,
    max_files_per_message: 16,
    perFileLabel: "25 MB",
  },
  pro: {
    label: "Pro",
    tier: "Pro",
    maxBytesPerFile: GIB,
    max_file_bytes: GIB,
    // The store remains capable of the established 1 GiB Pro transport
    // limit. Current native AEAD clients have a lower 512 MiB plaintext cap.
    maxSealedBytesPerFile: GIB,
    maxFilesPerMessage: 16,
    max_files_per_message: 16,
    perFileLabel: "1 GB",
  },
};

export type AttachmentLimit = AttachmentTierLimit;

export function attachmentTierLimit(tier: string): AttachmentTierLimit | null {
  return Object.hasOwn(ATTACHMENT_TIER_LIMITS, tier)
    ? ATTACHMENT_TIER_LIMITS[tier as AttachmentTier]
    : null;
}

export const attachmentLimitForTier = attachmentTierLimit;

function checkLimit(
  tier: AttachmentTier,
  sizeBytes: number,
  fileCount: number,
  sealed: boolean,
): AttachmentLimitCheck {
  const limit = ATTACHMENT_TIER_LIMITS[tier];
  const maximum = sealed ? limit.maxSealedBytesPerFile : limit.maxBytesPerFile;
  if (!Number.isSafeInteger(sizeBytes) || sizeBytes <= 0 || sizeBytes > maximum) {
    return {
      accepted: false,
      tier,
      tierLabel: limit.label,
      sizeBytes,
      fileCount,
      maxBytesPerFile: maximum,
      maxFilesPerMessage: limit.maxFilesPerMessage,
      reason: "file_too_large",
      firstRejectedFile: null,
    };
  }
  if (!Number.isSafeInteger(fileCount) || fileCount <= 0 || fileCount > limit.maxFilesPerMessage) {
    return {
      accepted: false,
      tier,
      tierLabel: limit.label,
      sizeBytes,
      fileCount,
      maxBytesPerFile: maximum,
      maxFilesPerMessage: limit.maxFilesPerMessage,
      reason: "too_many_files",
      firstRejectedFile: Number.isSafeInteger(fileCount) && fileCount > limit.maxFilesPerMessage
        ? limit.maxFilesPerMessage + 1
        : null,
    };
  }
  return {
    accepted: true,
    tier,
    tierLabel: limit.label,
    sizeBytes,
    fileCount,
    maxBytesPerFile: maximum,
    maxFilesPerMessage: limit.maxFilesPerMessage,
    reason: "accepted",
    firstRejectedFile: null,
  };
}

/** Product/plaintext limit check used by public limit reporting. */
export function checkAttachmentTierLimit(
  tier: AttachmentTier,
  sizeBytes: number,
  fileCount: number,
): AttachmentLimitCheck {
  return checkLimit(tier, sizeBytes, fileCount, false);
}

/** Store-side check for bytes that are already padded and authenticated. */
export function checkSealedAttachmentTierLimit(
  tier: AttachmentTier,
  sealedBytes: number,
  fileCount: number,
): AttachmentLimitCheck {
  return checkLimit(tier, sealedBytes, fileCount, true);
}

export type AttachmentLimitDecision =
  | { accepted: true; tier: "Free" | "Pro"; file_size_bytes: number; file_number: number }
  | {
      accepted: false;
      tier: "Free" | "Pro" | null;
      file_size_bytes: number;
      file_number: number;
      reason: "unknown_tier" | "invalid_size" | "too_large" | "invalid_file_number" | "too_many_files";
    };

export function checkAttachmentFileForTier(
  tier: string,
  fileSizeBytes: number,
  fileNumber: number,
): AttachmentLimitDecision {
  const limit = attachmentTierLimit(tier);
  if (!limit) {
    return { accepted: false, tier: null, file_size_bytes: fileSizeBytes, file_number: fileNumber, reason: "unknown_tier" };
  }
  if (!Number.isSafeInteger(fileSizeBytes) || fileSizeBytes <= 0) {
    return { accepted: false, tier: limit.tier, file_size_bytes: fileSizeBytes, file_number: fileNumber, reason: "invalid_size" };
  }
  if (fileSizeBytes > limit.maxBytesPerFile) {
    return { accepted: false, tier: limit.tier, file_size_bytes: fileSizeBytes, file_number: fileNumber, reason: "too_large" };
  }
  if (!Number.isSafeInteger(fileNumber) || fileNumber <= 0) {
    return { accepted: false, tier: limit.tier, file_size_bytes: fileSizeBytes, file_number: fileNumber, reason: "invalid_file_number" };
  }
  if (fileNumber > limit.maxFilesPerMessage) {
    return { accepted: false, tier: limit.tier, file_size_bytes: fileSizeBytes, file_number: fileNumber, reason: "too_many_files" };
  }
  return { accepted: true, tier: limit.tier, file_size_bytes: fileSizeBytes, file_number: fileNumber };
}

// Direct R2 buffering remains bounded at the Free plaintext limit. Larger
// sealed Free objects use multipart; tier admission uses the separate sealed
// ceiling above.
export const MAX_DIRECT_ATTACHMENT_BYTES = ATTACHMENT_TIER_LIMITS.free.maxBytesPerFile;
export const MAX_SEALED_ATTACHMENT_BYTES = ATTACHMENT_TIER_LIMITS.pro.maxSealedBytesPerFile;
export const MAX_ATTACHMENT_PART_BYTES = 8 * MIB;
export const MAX_ATTACHMENT_PARTS = Math.ceil(
  MAX_SEALED_ATTACHMENT_BYTES / MAX_ATTACHMENT_PART_BYTES,
);

export const MAX_LIVE_ATTACHMENT_ROWS = 512;
export const MAX_LIVE_ATTACHMENT_BYTES = 8 * GIB;

export const INCOMPLETE_SESSION_TTL_SECONDS = 15 * 60;
export const MAX_INCOMPLETE_SESSION_ROWS = 64;
export const MAX_INCOMPLETE_SESSION_BYTES = 2 * GIB;

export const ATTACHMENT_SWEEP_BATCH_SIZE = 100;
