export type AttachmentTier = "free" | "pro";

export interface AttachmentTierLimit {
  readonly label: string;
  readonly maxBytesPerFile: number;
  readonly maxFilesPerMessage: number;
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

export const ATTACHMENT_TIER_LIMITS: Record<AttachmentTier, AttachmentTierLimit> = {
  free: {
    label: "Free",
    maxBytesPerFile: 25 * 1024 * 1024,
    maxFilesPerMessage: 16,
    perFileLabel: "25 MB",
  },
  pro: {
    label: "Pro",
    maxBytesPerFile: 1024 * 1024 * 1024,
    maxFilesPerMessage: 16,
    perFileLabel: "1 GB",
  },
};

export function attachmentTierLimit(tier: string): AttachmentTierLimit | null {
export const ATTACHMENT_TIER_LIMITS = {
  free: {
    tier: "Free",
    max_file_bytes: 25 * 1024 * 1024,
    max_files_per_message: 16,
  },
  pro: {
    tier: "Pro",
    max_file_bytes: 1024 * 1024 * 1024,
    max_files_per_message: 16,
  },
} as const;

export type AttachmentTier = keyof typeof ATTACHMENT_TIER_LIMITS;
export type AttachmentLimit = typeof ATTACHMENT_TIER_LIMITS[AttachmentTier];

export type AttachmentLimitDecision =
  | { accepted: true; tier: AttachmentLimit["tier"]; file_size_bytes: number; file_number: number }
  | {
      accepted: false;
      tier: AttachmentLimit["tier"] | null;
      file_size_bytes: number;
      file_number: number;
      reason: "unknown_tier" | "invalid_size" | "too_large" | "invalid_file_number" | "too_many_files";
    };

export function attachmentLimitForTier(tier: string): AttachmentLimit | null {
  return Object.hasOwn(ATTACHMENT_TIER_LIMITS, tier)
    ? ATTACHMENT_TIER_LIMITS[tier as AttachmentTier]
    : null;
}

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
export function checkAttachmentFileForTier(
  tier: string,
  fileSizeBytes: number,
  fileNumber: number,
): AttachmentLimitDecision {
  const limit = attachmentLimitForTier(tier);
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
      tier: limit.tier,
      file_size_bytes: fileSizeBytes,
      file_number: fileNumber,
      reason: "invalid_size",
    };
  }
  if (fileSizeBytes > limit.max_file_bytes) {
    return {
      accepted: false,
      tier: limit.tier,
      file_size_bytes: fileSizeBytes,
      file_number: fileNumber,
      reason: "too_large",
    };
  }
  if (!Number.isSafeInteger(fileNumber) || fileNumber <= 0) {
    return {
      accepted: false,
      tier: limit.tier,
      file_size_bytes: fileSizeBytes,
      file_number: fileNumber,
      reason: "invalid_file_number",
    };
  }
  if (fileNumber > limit.max_files_per_message) {
    return {
      accepted: false,
      tier: limit.tier,
      file_size_bytes: fileSizeBytes,
      file_number: fileNumber,
      reason: "too_many_files",
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

export const MAX_DIRECT_ATTACHMENT_BYTES = ATTACHMENT_TIER_LIMITS.free.maxBytesPerFile;
export const MAX_SEALED_ATTACHMENT_BYTES = ATTACHMENT_TIER_LIMITS.pro.maxBytesPerFile;
    tier: limit.tier,
    file_size_bytes: fileSizeBytes,
    file_number: fileNumber,
  };
}

export const MAX_DIRECT_ATTACHMENT_BYTES = ATTACHMENT_TIER_LIMITS.free.max_file_bytes;
export const MAX_SEALED_ATTACHMENT_BYTES = ATTACHMENT_TIER_LIMITS.pro.max_file_bytes;
// Leaves a bounded allowance for chunk framing and AEAD tags without asking
// the store to infer plaintext size from opaque ciphertext.
export const MAX_SEALED_ATTACHMENT_BYTES = ATTACHMENT_TIER_LIMITS.pro.maxBytesPerFile;
export const MAX_ATTACHMENT_PART_BYTES = 8 * 1024 * 1024;
export const MAX_ATTACHMENT_PARTS = Math.ceil(
  MAX_SEALED_ATTACHMENT_BYTES / MAX_ATTACHMENT_PART_BYTES,
);

// Aggregate quota enforcement lives only in the Worker's `insertObject`
// conditional `INSERT INTO attachment_objects ... SELECT ... WHERE` statement.
// D1 serializes that single write statement; migration 0004 creates no trigger,
// and there is no trigger or CHECK constraint for the row/byte aggregates.
export const MAX_LIVE_ATTACHMENT_ROWS = 512;
export const MAX_LIVE_ATTACHMENT_BYTES = 8 * 1024 * 1024 * 1024;

// --- Incomplete multipart sessions (audit HIGH-1) -------------------------
//
// A session reserves its DECLARED size before any ciphertext exists, so
// reservations must live in their own small pool rather than competing with
// stored content. The numbers below are chosen so that an attacker who fills
// the reservation pool completely still leaves 448 rows and 6 GiB available to
// ordinary uploads, and so that the damage self-heals within one sweep cycle
// of the hold expiring.

/// How long an unfinished session may hold its reservation. Each accepted part
/// slides this forward, so a slow but genuine upload is never cut off; only a
/// session that stops making progress is reclaimed.
export const INCOMPLETE_SESSION_TTL_SECONDS = 15 * 60;

/// Reservation-pool ceilings, applied only to rows whose state is not `ready`.
/// Two concurrent full-size (1 GiB) uploads fit; the global backstop above
/// still applies on top.
export const MAX_INCOMPLETE_SESSION_ROWS = 64;
export const MAX_INCOMPLETE_SESSION_BYTES = 2 * 1024 * 1024 * 1024;

export const ATTACHMENT_SWEEP_BATCH_SIZE = 100;
