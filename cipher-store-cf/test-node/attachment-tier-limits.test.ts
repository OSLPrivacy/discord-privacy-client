import { describe, expect, it } from "vitest";
import {
  ATTACHMENT_TIER_LIMITS,
  attachmentLimitForTier,
  MAX_DIRECT_ATTACHMENT_BYTES,
  MAX_SEALED_ATTACHMENT_BYTES,
} from "../src/lib/attachment-limits.js";

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
});
