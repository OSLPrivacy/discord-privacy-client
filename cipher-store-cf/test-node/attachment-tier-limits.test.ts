import { describe, expect, it } from "vitest";
import {
  ATTACHMENT_TIER_LIMITS,
  checkAttachmentTierLimit,
} from "../src/lib/attachment-limits.js";

const mib = 1024 * 1024;
const gib = 1024 * 1024 * 1024;

function verdict(sizeLabel: string, tier: "free" | "pro", fileCount: number): string {
  const sizeBytes = sizeLabel === "1.1 GB"
    ? 1.1 * gib
    : Number(sizeLabel.split(" ")[0]) * (sizeLabel.endsWith("GB") ? gib : mib);
  const result = checkAttachmentTierLimit(tier, sizeBytes, fileCount);
  if (result.accepted) return `accept ${sizeLabel} ${result.tierLabel}`;
  if (result.reason === "too_many_files" && result.firstRejectedFile !== null) {
    return `reject file ${result.firstRejectedFile} ${result.tierLabel}`;
  }
  return `reject ${sizeLabel} ${result.tierLabel}`;
}

describe("attachment tier limits", () => {
  it("defines Free and Pro file-size limits with one shared file-count record", () => {
    expect(ATTACHMENT_TIER_LIMITS.free).toMatchObject({
      label: "Free",
      maxBytesPerFile: 25 * mib,
      maxFilesPerMessage: 16,
    });
    expect(ATTACHMENT_TIER_LIMITS.pro).toMatchObject({
      label: "Pro",
      maxBytesPerFile: gib,
      maxFilesPerMessage: 16,
    });
  });

  it("prints the direct call finish-line verdicts", () => {
    const lines = [
      verdict("24 MB", "free", 16),
      verdict("999 MB", "pro", 16),
      verdict("26 MB", "free", 1),
      verdict("1.1 GB", "pro", 1),
      verdict("1 MB", "free", 17),
    ];
    for (const line of lines) console.log(line);
    expect(lines).toEqual([
      "accept 24 MB Free",
      "accept 999 MB Pro",
      "reject 26 MB Free",
      "reject 1.1 GB Pro",
      "reject file 17 Free",
    ]);
  });
});
