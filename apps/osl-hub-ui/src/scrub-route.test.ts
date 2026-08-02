import { describe, expect, it } from "vitest";
import { furthestScrubRouteStep, renderScrubRoute, scrubRouteStep, type ScrubRouteState } from "./scrub-route";

const fixture: ScrubRouteState = {
  accounts: [{ id: "mail", label: "Personal mail", detail: "Local export" }],
  selectedAccountIds: ["mail"],
  selectedCategories: ["personal"],
  scan: { state: "not-started", findings: 0 },
};

describe("scrub route", () => {
  it("renders each route step from fixture state", () => {
    expect(renderScrubRoute({ ...fixture, selectedAccountIds: [] }, "choose")).toContain("Choose what to scan");
    expect(renderScrubRoute(fixture, "scan")).toContain("Scan selected content");
    expect(renderScrubRoute({ ...fixture, scan: { state: "complete", findings: 2 } }, "review")).toContain("2 items are ready for your review");
  });

  it("does not allow a step to be skipped", () => {
    const noScope = { ...fixture, selectedAccountIds: [], scan: { state: "complete" as const, findings: 2 } };
    expect(furthestScrubRouteStep(noScope)).toBe("choose");
    expect(scrubRouteStep(noScope, "review")).toBe("choose");
    expect(scrubRouteStep(fixture, "review")).toBe("scan");
    expect(renderScrubRoute(fixture, "review")).toContain("Scan selected content");
  });

  it("keeps review disabled until the scan is complete", () => {
    expect(renderScrubRoute(fixture, "scan")).toContain('data-scrub-route-next="review" disabled');
    expect(renderScrubRoute({ ...fixture, scan: { state: "complete", findings: 0 } }, "scan")).not.toContain('data-scrub-route-next="review" disabled');
  });
});
