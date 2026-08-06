import {
  ATTACHMENT_TIER_LIMITS,
  attachmentTierLimit,
  checkAttachmentTierLimit,
  type AttachmentTier,
} from "../src/lib/attachment-limits.ts";

const [, , commandOrTier, maybeSize, maybeCount] = process.argv;

function parseSizeBytes(raw: string): number | null {
  const match = raw.trim().match(/^(\d+(?:\.\d+)?)\s*(MB|GB)$/i);
  if (!match) return null;
  const amount = Number(match[1]);
  if (!Number.isFinite(amount) || amount <= 0) return null;
  const unit = match[2]!.toUpperCase();
  const multiplier = unit === "GB" ? 1024 * 1024 * 1024 : 1024 * 1024;
  const bytes = Math.ceil(amount * multiplier);
  return Number.isSafeInteger(bytes) ? bytes : null;
}

function isAttachmentTier(raw: string): raw is AttachmentTier {
  return attachmentTierLimit(raw) !== null;
}

function requireTier(raw: string | undefined): AttachmentTier {
  if (!raw || !isAttachmentTier(raw)) {
    console.error(`attachment tier ${raw ?? ""} has no limit`);
    process.exit(1);
  }
  return raw;
}

function runCheck(): void {
  const tier = requireTier(maybeSize);
  if (!maybeCount) {
    console.error("attachment size is required");
    process.exit(1);
  }
  const sizeBytes = parseSizeBytes(maybeCount);
  if (sizeBytes === null) {
    console.error(`attachment size ${maybeCount} is invalid`);
    process.exit(1);
  }
  const fileCount = Number(process.argv[5]);
  if (!Number.isSafeInteger(fileCount)) {
    console.error(`attachment file count ${process.argv[5] ?? ""} is invalid`);
    process.exit(1);
  }

  const result = checkAttachmentTierLimit(tier, sizeBytes, fileCount);
  if (result.accepted) {
    console.log(`accept ${maybeCount} ${result.tierLabel}`);
    return;
  }
  if (result.reason === "too_many_files" && result.firstRejectedFile !== null) {
    console.log(`reject file ${result.firstRejectedFile} ${result.tierLabel}`);
    return;
  }
  console.log(`reject ${maybeCount} ${result.tierLabel}`);
}

function listTier(raw: string | undefined): void {
  const tier = requireTier(raw);
  const limit = ATTACHMENT_TIER_LIMITS[tier];
  console.log(`${limit.label}: ${limit.perFileLabel} per file; ${limit.maxFilesPerMessage} files per message`);
}

if (commandOrTier === "check") {
  runCheck();
} else {
  for (const rawTier of process.argv.slice(2)) {
    listTier(rawTier);
  }
}
