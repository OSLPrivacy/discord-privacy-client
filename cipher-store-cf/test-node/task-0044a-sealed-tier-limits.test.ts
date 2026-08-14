import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  ATTACHMENT_TIER_LIMITS,
  attachmentTierLimit,
  checkAttachmentTierLimit,
  checkSealedAttachmentTierLimit,
} from "../src/lib/attachment-limits.js";

describe("TASK 0044a sealed attachment tier admission", () => {
  it("reconciles exactly Free and Pro while keeping plaintext and sealed limits distinct", () => {
    expect(Object.keys(ATTACHMENT_TIER_LIMITS).sort()).toEqual(["free", "pro"]);
    expect(attachmentTierLimit("enterprise")).toBeNull();
    expect(checkAttachmentTierLimit("free", 25 * 1024 * 1024, 1).accepted).toBe(true);
    expect(checkAttachmentTierLimit("free", 25 * 1024 * 1024 + 1, 1).accepted).toBe(false);

    const freeSealedMaximum = ATTACHMENT_TIER_LIMITS.free.maxSealedBytesPerFile;
    expect(checkSealedAttachmentTierLimit("free", freeSealedMaximum, 1).accepted).toBe(true);
    expect(checkSealedAttachmentTierLimit("free", freeSealedMaximum + 1, 1).accepted).toBe(false);
    expect(checkSealedAttachmentTierLimit("pro", 1024 * 1024 * 1024, 1).accepted).toBe(true);
    console.log(
      `TASK0044A worker_tiers=free:Free,pro:Pro free_plaintext_max=${ATTACHMENT_TIER_LIMITS.free.maxBytesPerFile} free_sealed_max=${freeSealedMaximum} pro_plaintext_max=${ATTACHMENT_TIER_LIMITS.pro.maxBytesPerFile} pro_sealed_max=${ATTACHMENT_TIER_LIMITS.pro.maxSealedBytesPerFile}`,
    );
  });

  it("uses sealed-byte tier admission at both deployed Worker upload entry points", () => {
    const endpoint = readFileSync(new URL("../src/endpoints/attachment.ts", import.meta.url), "utf8");
    expect(endpoint).toContain("checkSealedAttachmentTierLimit");
    expect(endpoint.match(/const tier = readAccountTier\(request\);/gu)).toHaveLength(2);
    expect(endpoint.match(/const tierRejected = enforceAccountTier\(tier,/gu)).toHaveLength(2);
    console.log("TASK0044A worker_entrypoints=direct,multipart tier_admission=sealed-bytes");
  });
});
