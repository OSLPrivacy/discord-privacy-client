import {
  ATTACHMENT_TIER_LIMITS,
  checkAttachmentFileForTier,
  attachmentLimitForTier,
  type AttachmentTier,
} from "../src/lib/attachment-limits.ts";

export function formatBytes(bytes: number): string {
  if (bytes % (1024 * 1024 * 1024) === 0) {
    return `${bytes / (1024 * 1024 * 1024)} GB`;
  }
  if (bytes % (1024 * 1024) === 0) {
    return `${bytes / (1024 * 1024)} MB`;
  }
  return `${bytes} bytes`;
}

function parseDecimalBytes(raw: string): number {
  const normalized = raw.trim().toLowerCase();
  const match = /^(\d+(?:\.\d+)?)\s*(mb|gb)$/.exec(normalized);
  if (!match) throw new Error(`attachment size ${raw} is invalid`);
  const value = Number(match[1]);
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`attachment size ${raw} is invalid`);
  }
  const unit = match[2];
  const multiplier = unit === "gb" ? 1024 * 1024 * 1024 : 1024 * 1024;
  const bytes = Math.ceil(value * multiplier);
  if (!Number.isSafeInteger(bytes)) {
    throw new Error(`attachment size ${raw} is invalid`);
  }
  return bytes;
}

function printLimit(tier: AttachmentTier): void {
  const limit = attachmentLimitForTier(tier);
  if (!limit) {
    throw new Error(`attachment tier ${tier} has no limit`);
  }
  console.log(
    `${limit.tier}: ${formatBytes(limit.max_file_bytes)} per file; `
    + `${limit.max_files_per_message} files per message`,
  );
}

function printCheck(rawTier: string, rawSize: string, rawFileNumber: string): void {
  const fileNumber = Number(rawFileNumber);
  if (!Number.isSafeInteger(fileNumber)) {
    throw new Error(`attachment file number ${rawFileNumber} is invalid`);
  }
  const decision = checkAttachmentFileForTier(rawTier, parseDecimalBytes(rawSize), fileNumber);
  const tier = decision.tier ?? rawTier;
  const prefix = decision.accepted ? "accept" : "reject";
  if (!decision.accepted && decision.reason === "too_many_files") {
    console.log(`${prefix} file ${decision.file_number} ${tier}`);
    return;
  }
  console.log(`${prefix} ${rawSize} ${tier}`);
}

const requested = process.argv.slice(2);

try {
  if (requested[0] === "check") {
    if (requested.length !== 4) {
      throw new Error("usage: attachment-tier-limits.ts check <tier> <size> <file-number>");
    }
    printCheck(requested[1]!, requested[2]!, requested[3]!);
  } else {
    const tiers = requested.length > 0
      ? requested
      : Object.keys(ATTACHMENT_TIER_LIMITS);
    for (const tier of tiers) {
      const limit = attachmentLimitForTier(tier);
      if (!limit) {
        throw new Error(`attachment tier ${tier} has no limit`);
      }
      printLimit(tier as AttachmentTier);
    }
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
