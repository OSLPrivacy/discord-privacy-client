import { describe, expect, it } from "vitest";
import type { ScrubCoverageReceipt } from "./scrub-plan";
import { scrubCoverageReceiptMarkup } from "./scrub-coverage-view";

function receipt(overrides: Partial<ScrubCoverageReceipt> = {}): ScrubCoverageReceipt {
  return {
    targetId: "discord:account-1",
    messagesScanned: 24,
    oldestReachableUnixMs: 1_700_000_000_000,
    newestReachableUnixMs: 1_700_086_400_000,
    providerReportedComplete: true,
    gaps: [],
    textChecked: true,
    imagesChecked: true,
    ...overrides,
  };
}

describe("Scrub coverage receipt", () => {
  it("says what was not seen when the provider reports a partial index", () => {
    const markup = scrubCoverageReceiptMarkup(receipt({
      providerReportedComplete: false,
      gaps: ["The provider stopped loading older messages."],
    }));

    expect(markup).toContain("We did not see everything.");
    expect(markup).toContain("The provider stopped loading older messages.");
    expect(markup).toContain("We inspected 24 messages, not a complete account history.");
    expect(markup).not.toMatch(/all 24 messages|complete account history was scanned/i);
  });

  it("only calls coverage complete when the provider attests it and there are no gaps", () => {
    const markup = scrubCoverageReceiptMarkup(receipt());

    expect(markup).toContain("The provider reported this coverage as complete.");
    expect(markup).toContain("We inspected 24 messages.");
  });
});
