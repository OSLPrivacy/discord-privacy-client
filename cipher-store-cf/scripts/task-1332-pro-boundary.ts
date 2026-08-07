import {
  ATTACHMENT_TIER_LIMITS,
  checkAttachmentTierLimit,
  type AttachmentTier,
} from "../src/lib/attachment-limits.ts";

function usage(): never {
  console.error("usage: task-1332-pro-boundary.ts pro <size-bytes>");
  process.exit(2);
}

const [, , rawTier, rawSizeBytes] = process.argv;
if (rawTier !== "pro") usage();

const tier: AttachmentTier = rawTier;
if (!rawSizeBytes || !/^\d+$/.test(rawSizeBytes)) usage();

const sizeBytes = Number(rawSizeBytes);
if (!Number.isSafeInteger(sizeBytes) || sizeBytes <= 0) usage();

const result = checkAttachmentTierLimit(tier, sizeBytes, 1);
const limit = ATTACHMENT_TIER_LIMITS[tier];
const exitCode = result.accepted ? 0 : 1;

console.log(
  [
    "task1332",
    "direct=checkAttachmentTierLimit",
    `tier=${tier}`,
    `size_bytes=${sizeBytes}`,
    `limit_bytes=${limit.maxBytesPerFile}`,
    `limit_label=${limit.perFileLabel}`,
    `result=${result.accepted ? "accepted" : "rejected"}`,
    `reason=${result.reason}`,
    `exit=${exitCode}`,
  ].join(" "),
);

process.exit(exitCode);
