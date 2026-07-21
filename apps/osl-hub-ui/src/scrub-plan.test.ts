import { describe, expect, it } from "vitest";
import { defaultScrubSignalGroups } from "./scrub";
import { defaultScrubSetupPlan, parseScrubSetupPlan, validateCoverageReceipt } from "./scrub-plan";

describe("Scrub setup and coverage contracts", () => {
  const targets = new Set(["discord:current-session", "telegram:current-session"]);

  it("defaults to an explicit skip and all review categories", () => {
    expect(defaultScrubSetupPlan(defaultScrubSignalGroups)).toEqual({
      mode: "skip",
      targetIds: [],
      signalGroups: [...defaultScrubSignalGroups],
    });
  });

  it("keeps only exact selected accounts and gates AutoScrub to Pro", () => {
    const raw = JSON.stringify({
      mode: "autoscrub",
      targetIds: [...targets],
      signalGroups: ["language", "personal"],
    });
    expect(parseScrubSetupPlan(raw, targets, defaultScrubSignalGroups, true).mode).toBe("autoscrub");
    expect(parseScrubSetupPlan(raw, targets, defaultScrubSignalGroups, false).mode).toBe("scrub");
    expect(parseScrubSetupPlan(raw.replace("current-session", "other"), targets, defaultScrubSignalGroups, true).mode).toBe("skip");
  });

  it("does not call history complete when a provider reports gaps", () => {
    const complete = {
      targetId: "discord:current-session",
      messagesScanned: 42,
      oldestReachableUnixMs: 1,
      newestReachableUnixMs: 2,
      providerReportedComplete: true,
      gaps: [],
      textChecked: true,
      imagesChecked: false,
    };
    expect(validateCoverageReceipt(complete, targets)).toEqual(complete);
    expect(validateCoverageReceipt({ ...complete, gaps: ["Older messages unavailable"] }, targets)).toBeNull();
  });
});
