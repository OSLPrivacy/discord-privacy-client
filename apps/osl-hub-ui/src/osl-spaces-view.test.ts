import { describe, expect, it } from "vitest";

import { oslSpaceStateMarkup } from "./osl-spaces-view";

describe("Space honest states", () => {
  it("keeps a queued send pending while offline", () => {
    const markup = oslSpaceStateMarkup({ offline: true, queuedSends: 1 });

    expect(markup).toContain("osl-space-state--offline");
    expect(markup).toContain("1 message is waiting to send when you're back online.");
    expect(markup).not.toContain("delivered");
  });

  it("separates every outstanding Space condition from a completed state", () => {
    const markup = oslSpaceStateMarkup({
      staleRoster: true,
      burnRequestsQueued: 2,
      removalUnconfirmed: true,
      acknowledgementsOutstanding: 3,
    });

    expect(markup).toContain("osl-space-state--stale-roster");
    expect(markup).toContain("Posting is unavailable until membership is current.");
    expect(markup).toContain("osl-space-state--burn-queued");
    expect(markup).toContain("2 removal requests are queued for the server.");
    expect(markup).toContain("osl-space-state--removal-unconfirmed");
    expect(markup).toContain("may retain content they already have");
    expect(markup).toContain("osl-space-state--ack-unconfirmed");
    expect(markup).toContain("3 acknowledgements are still outstanding.");
    expect(markup).not.toContain(">Confirmed<");
    expect(markup).not.toContain("delivered");
    expect(markup).not.toContain("style=");
  });
});
