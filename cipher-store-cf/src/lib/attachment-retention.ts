import {
  ATTACHMENT_TIER_LIMITS,
  attachmentTierLimit,
  type AttachmentTier,
} from "./attachment-limits.js";

export const ATTACHMENT_RETENTION_SECONDS: Record<AttachmentTier, number> = {
  free: 7 * 24 * 60 * 60,
  pro: 30 * 24 * 60 * 60,
};

export function parseAttachmentTier(raw: string | null): AttachmentTier | null {
  const tier = raw?.trim().toLowerCase() ?? "free";
  return attachmentTierLimit(tier) === null ? null : tier as AttachmentTier;
}

export function attachmentRetentionLimitMessage(
  tier: AttachmentTier,
  requestedSeconds: number,
): string | null {
  const limit = ATTACHMENT_RETENTION_SECONDS[tier];
  if (requestedSeconds <= limit) return null;
  const days = limit / (24 * 60 * 60);
  return `${ATTACHMENT_TIER_LIMITS[tier].label} attachments have a ${days}-day limit (${limit} seconds)`;
}
