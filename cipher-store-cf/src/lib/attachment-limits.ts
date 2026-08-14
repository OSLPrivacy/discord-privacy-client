export type AttachmentTier = "free" | "pro";

export interface AttachmentTierLimit {
  readonly label: "Free" | "Pro";
  /** Backward-compatible display label used by the direct CLI checker. */
  readonly tier: "Free" | "Pro";
  readonly maxBytesPerFile: number;
  readonly max_file_bytes: number;
  readonly maxFilesPerMessage: number;
  readonly max_files_per_message: number;
  readonly perFileLabel: "25 MB" | "1 GB";
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

const freeBytes = 25 * 1024 * 1024;
const proBytes = 1024 * 1024 * 1024;

export const ATTACHMENT_TIER_LIMITS: Record<AttachmentTier, AttachmentTierLimit> = {
  free: {
    label: "Free",
    tier: "Free",
    maxBytesPerFile: freeBytes,
    max_file_bytes: freeBytes,
    maxFilesPerMessage: 16,
    max_files_per_message: 16,
    perFileLabel: "25 MB",
  },
  pro: {
    label: "Pro",
    tier: "Pro",
    maxBytesPerFile: proBytes,
    max_file_bytes: proBytes,
    maxFilesPerMessage: 16,
    max_files_per_message: 16,
    perFileLabel: "1 GB",
  },
};

export function attachmentTierLimit(tier: string): AttachmentTierLimit | null {
  return Object.hasOwn(ATTACHMENT_TIER_LIMITS, tier)
    ? ATTACHMENT_TIER_LIMITS[tier as AttachmentTier]
    : null;
}

/** Historical alias retained for the command-line limit checker. */
export const attachmentLimitForTier = attachmentTierLimit;

export function checkAttachmentTierLimit(
  tier: AttachmentTier,
  sizeBytes: number,
  fileCount: number,
): AttachmentLimitCheck {
  const limit = ATTACHMENT_TIER_LIMITS[tier];
  if (!Number.isSafeInteger(sizeBytes) || sizeBytes <= 0 || sizeBytes > limit.maxBytesPerFile) {
    return {
      accepted: false,
      tier,
      tierLabel: limit.label,
      sizeBytes,
      fileCount,
      maxBytesPerFile: limit.maxBytesPerFile,
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
      maxBytesPerFile: limit.maxBytesPerFile,
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
    maxBytesPerFile: limit.maxBytesPerFile,
    maxFilesPerMessage: limit.maxFilesPerMessage,
    reason: "accepted",
    firstRejectedFile: null,
  };
}

export type AttachmentLimitDecision =
  | { accepted: true; tier: string; file_size_bytes: number; file_number: number }
  | {
      accepted: false;
      tier: string | null;
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
    return {
      accepted: false,
      tier: null,
      file_size_bytes: fileSizeBytes,
      file_number: fileNumber,
      reason: "unknown_tier",
    };
  }
  if (!Number.isSafeInteger(fileSizeBytes) || fileSizeBytes <= 0) {
    return {
      accepted: false,
      tier: limit.label,
      file_size_bytes: fileSizeBytes,
      file_number: fileNumber,
      reason: "invalid_size",
    };
  }
  if (fileSizeBytes > limit.maxBytesPerFile) {
    return {
      accepted: false,
      tier: limit.label,
      file_size_bytes: fileSizeBytes,
      file_number: fileNumber,
      reason: "too_large",
    };
  }
  if (!Number.isSafeInteger(fileNumber) || fileNumber <= 0) {
    return {
      accepted: false,
      tier: limit.label,
      file_size_bytes: fileSizeBytes,
      file_number: fileNumber,
      reason: "invalid_file_number",
    };
  }
  if (fileNumber > limit.maxFilesPerMessage) {
    return {
      accepted: false,
      tier: limit.label,
      file_size_bytes: fileSizeBytes,
      file_number: fileNumber,
      reason: "too_many_files",
    };
  }
  return {
    accepted: true,
    tier: limit.label,
    file_size_bytes: fileSizeBytes,
    file_number: fileNumber,
  };
}

export const MAX_DIRECT_ATTACHMENT_BYTES = ATTACHMENT_TIER_LIMITS.free.maxBytesPerFile;
export const MAX_SEALED_ATTACHMENT_BYTES = ATTACHMENT_TIER_LIMITS.pro.maxBytesPerFile;
// Leaves a bounded allowance for chunk framing and AEAD tags without asking
// the store to infer plaintext size from opaque ciphertext.
export const MAX_ATTACHMENT_PART_BYTES = 8 * 1024 * 1024;
export const MAX_ATTACHMENT_PARTS = Math.ceil(
  MAX_SEALED_ATTACHMENT_BYTES / MAX_ATTACHMENT_PART_BYTES,
);

// Aggregate quota enforcement lives only in the Worker's `insertObject`
// conditional INSERT. D1 serializes that statement.
export const MAX_LIVE_ATTACHMENT_ROWS = 512;
export const MAX_LIVE_ATTACHMENT_BYTES = 8 * 1024 * 1024 * 1024;

export const INCOMPLETE_SESSION_TTL_SECONDS = 15 * 60;
export const MAX_INCOMPLETE_SESSION_ROWS = 64;
export const MAX_INCOMPLETE_SESSION_BYTES = 2 * 1024 * 1024 * 1024;
export const ATTACHMENT_SWEEP_BATCH_SIZE = 100;
