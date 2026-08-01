import { describe, expect, it } from "vitest";
import { renderScrubReceipt, scrubReceiptCounts } from "./scrub-receipt-view";
import type { ProviderDeletionReceipt } from "./scrub-delete-engine";

const receipt: ProviderDeletionReceipt = {
  providerId: "provider",
  accountId: "account",
  dryRun: false,
  consentId: "consent",
  startedAt: 1,
  completedAt: 2,
  stoppedFailClosed: true,
  items: [
    { providerId: "provider", accountId: "account", channelId: "channel", itemId: "one", outcome: "confirmed-deleted", deletionCalled: true, verifiedByReadback: true, detail: "Independent recheck found it absent." },
    { providerId: "provider", accountId: "account", channelId: "channel", itemId: "two", outcome: "confirmed-not-deleted", deletionCalled: true, verifiedByReadback: true, detail: "The provider still shows it." },
    { providerId: "provider", accountId: "account", channelId: "channel", itemId: "three", outcome: "UNKNOWN", deletionCalled: true, verifiedByReadback: false, detail: "Verification connection dropped." },
  ],
};

describe("Scrub receipt view", () => {
  it("renders three separate visible counts and never treats Unknown as deleted", () => {
    expect(scrubReceiptCounts(receipt.items)).toEqual({ deleted: 1, stillPresent: 1, unknown: 1 });

    const markup = renderScrubReceipt(receipt);
    expect(markup).toContain('aria-label="1 deleted"');
    expect(markup).toContain('aria-label="1 still present"');
    expect(markup).toContain('aria-label="1 Unknown"');
    expect(markup).not.toContain('aria-label="2 deleted"');
  });

  it("gives every Unknown outcome its reason and a manual-resolution link", () => {
    const markup = renderScrubReceipt(receipt);

    expect(markup).toContain("Verification connection dropped.");
    expect(markup).toContain('href="#scrub-manual-resolution"');
    expect(markup).toContain("Resolve manually");
  });
});
