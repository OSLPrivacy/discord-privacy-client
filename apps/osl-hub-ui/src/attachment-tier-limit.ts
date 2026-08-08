export type AttachmentTierAccess = "free" | "pro" | "offlineGrace";

export type AttachmentTierLimit = {
  tierLabel: "Free" | "Pro";
  perFileLimitLabel: "25 MB" | "1 GB";
};

export function attachmentTierLimit(access: AttachmentTierAccess): AttachmentTierLimit {
  return access === "free"
    ? { tierLabel: "Free", perFileLimitLabel: "25 MB" }
    : { tierLabel: "Pro", perFileLimitLabel: "1 GB" };
}
