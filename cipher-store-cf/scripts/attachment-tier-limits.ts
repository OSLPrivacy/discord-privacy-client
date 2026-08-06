import {
  ATTACHMENT_TIER_LIMITS,
  attachmentLimitForTier,
  type AttachmentTier,
} from "../src/lib/attachment-limits.ts";

function formatBytes(bytes: number): string {
  if (bytes % (1024 * 1024 * 1024) === 0) {
    return `${bytes / (1024 * 1024 * 1024)} GB`;
  }
  if (bytes % (1024 * 1024) === 0) {
    return `${bytes / (1024 * 1024)} MB`;
  }
  return `${bytes} bytes`;
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

const requested = process.argv.slice(2);
const tiers = requested.length > 0
  ? requested
  : Object.keys(ATTACHMENT_TIER_LIMITS);

try {
  for (const tier of tiers) {
    const limit = attachmentLimitForTier(tier);
    if (!limit) {
      throw new Error(`attachment tier ${tier} has no limit`);
    }
    printLimit(tier as AttachmentTier);
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
