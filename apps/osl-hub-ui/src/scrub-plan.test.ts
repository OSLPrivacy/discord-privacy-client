import { describe, expect, it } from "vitest";
import { defaultScrubSignalGroups } from "./scrub";
import { parseScrubSetupPlan, SCRUB_TARGET_LIMIT, targetId, validateCoverageReceipt } from "./scrub-plan";

const targets = new Set([targetId("discord", "personal"), targetId("signal", "work")]);

describe("Scrub setup plan", () => {
  it("uses safe defaults for missing or malformed plans", () => {
    expect(parseScrubSetupPlan(null, targets, defaultScrubSignalGroups, false)).toEqual({
      mode: "scrub",
      targetIds: [],
      signalGroups: defaultScrubSignalGroups,
    });
    expect(parseScrubSetupPlan("not-json", targets, defaultScrubSignalGroups, false).mode).toBe("scrub");
  });

  it("keeps only available targets and allowed signal groups", () => {
    const parsed = parseScrubSetupPlan(JSON.stringify({
      mode: "skip",
      targetIds: [targetId("discord", "personal"), "unknown:account", targetId("discord", "personal")],
      signalGroups: [defaultScrubSignalGroups[0], "unknown", defaultScrubSignalGroups[0]],
    }), targets, defaultScrubSignalGroups, true);
    expect(parsed).toEqual({
      mode: "skip",
      targetIds: [targetId("discord", "personal")],
      signalGroups: [defaultScrubSignalGroups[0]],
    });
  });

  it("downgrades AutoScrub without Pro and caps targets", () => {
    const manyTargets = new Set(Array.from({ length: SCRUB_TARGET_LIMIT + 4 }, (_, index) => targetId("discord", `account-${index}`)));
    const raw = JSON.stringify({ mode: "autoscrub", targetIds: [...manyTargets], signalGroups: defaultScrubSignalGroups });
    expect(parseScrubSetupPlan(raw, manyTargets, defaultScrubSignalGroups, false).mode).toBe("scrub");
    expect(parseScrubSetupPlan(raw, manyTargets, defaultScrubSignalGroups, true).targetIds).toHaveLength(SCRUB_TARGET_LIMIT);
  });

  it("validates honest text-only coverage and never permits complete coverage with gaps", () => {
    const receipt = {
      messagesScanned: 2,
      oldestReachableAtUnixMs: 1_700_000_000_000,
      newestReachableAtUnixMs: 1_700_000_100_000,
      providerReportedComplete: false,
      gaps: ["The provider did not attest that this export is complete."],
      textChecked: true,
      imagesChecked: false,
      videosChecked: false,
      attachmentsScanned: 1,
      attachmentTypesScanned: ["plain_text"],
      uninspectedAttachments: [],
    };
    expect(validateCoverageReceipt(receipt)).toBe(true);
    expect(validateCoverageReceipt({ ...receipt, providerReportedComplete: true })).toBe(false);
    expect(validateCoverageReceipt({ ...receipt, imagesChecked: true })).toBe(true);
    expect(validateCoverageReceipt({ ...receipt, gaps: [], providerReportedComplete: true, uninspectedAttachments: [{
      attachmentId: "photo", path: "photo.png", detectedType: "png", reason: "model_not_installed", detail: "Install the verified local image model pack.",
    }] })).toBe(false);
    expect(validateCoverageReceipt({ ...receipt, oldestReachableAtUnixMs: receipt.newestReachableAtUnixMs + 1 })).toBe(false);
  });
});
