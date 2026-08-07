import { describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  ATTACHMENT_TIER_LIMITS,
  checkAttachmentFileForTier,
  attachmentLimitForTier,
  MAX_DIRECT_ATTACHMENT_BYTES,
  MAX_SEALED_ATTACHMENT_BYTES,
} from "../src/lib/attachment-limits.js";

const packageRoot = fileURLToPath(new URL("..", import.meta.url));

function directLimitCheck(...args: string[]): string {
  return execFileSync(
    process.execPath,
    ["scripts/attachment-tier-limits.ts", "check", ...args],
    { cwd: packageRoot, encoding: "utf8" },
  ).trim();
}

describe("attachment tier limits", () => {
  it("keeps free and pro per-file limits with one shared per-message count", () => {
    expect(ATTACHMENT_TIER_LIMITS.free).toMatchObject({
      tier: "Free",
      max_file_bytes: 25 * 1024 * 1024,
      max_files_per_message: 16,
    });
    expect(ATTACHMENT_TIER_LIMITS.pro).toMatchObject({
      tier: "Pro",
      max_file_bytes: 1024 * 1024 * 1024,
      max_files_per_message: 16,
    });
    expect(ATTACHMENT_TIER_LIMITS.free.max_files_per_message)
      .toBe(ATTACHMENT_TIER_LIMITS.pro.max_files_per_message);
    expect(MAX_DIRECT_ATTACHMENT_BYTES).toBe(ATTACHMENT_TIER_LIMITS.free.max_file_bytes);
    expect(MAX_SEALED_ATTACHMENT_BYTES).toBe(ATTACHMENT_TIER_LIMITS.pro.max_file_bytes);
  });

  it("fails closed when a tier has no attachment limit", () => {
    expect(attachmentLimitForTier("enterprise")).toBeNull();
  });

  it("accepts files only when their size and count fit the sender tier", () => {
    expect(checkAttachmentFileForTier("free", 24 * 1024 * 1024, 16)).toMatchObject({
      accepted: true,
      tier: "Free",
      file_size_bytes: 24 * 1024 * 1024,
      file_number: 16,
    });
    expect(checkAttachmentFileForTier("pro", 999 * 1024 * 1024, 16)).toMatchObject({
      accepted: true,
      tier: "Pro",
      file_size_bytes: 999 * 1024 * 1024,
      file_number: 16,
    });
    expect(checkAttachmentFileForTier("free", 26 * 1024 * 1024, 1)).toMatchObject({
      accepted: false,
      tier: "Free",
      reason: "too_large",
    });
    expect(checkAttachmentFileForTier("pro", Math.ceil(1.1 * 1024 * 1024 * 1024), 1))
      .toMatchObject({
        accepted: false,
        tier: "Pro",
        reason: "too_large",
      });
    expect(checkAttachmentFileForTier("free", 1 * 1024 * 1024, 17)).toMatchObject({
      accepted: false,
      tier: "Free",
      file_number: 17,
      reason: "too_many_files",
    });
  });

  it("prints direct attachment limit check decisions", () => {
    expect(directLimitCheck("free", "24 MB", "16")).toBe("accept 24 MB Free");
    expect(directLimitCheck("pro", "999 MB", "16")).toBe("accept 999 MB Pro");
    expect(directLimitCheck("free", "26 MB", "1")).toBe("reject 26 MB Free");
    expect(directLimitCheck("pro", "1.1 GB", "1")).toBe("reject 1.1 GB Pro");
    expect(directLimitCheck("free", "1 MB", "17")).toBe("reject file 17 Free");
  });
});
