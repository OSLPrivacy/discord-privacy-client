import { beforeEach, describe, expect, it, vi } from "vitest";
import type { OslMailStage } from "./desktop-service-policy";

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

const reviewedStageC: OslMailStage = {
  id: "stageC",
  label: "Stage C - OSL mailbox",
  availability: "available",
  boundary: "Full mailbox only after a separate mail operations review.",
  includes: ["optional OSL address", "custom domains", "calendar", "encrypted storage"],
  excludes: [],
  externalEmailScope: "ordinaryExternalEmailUnlessSeparatelySupported",
};

const acceptedReview = {
  mailOperationsReviewAccepted: true,
  explicitConsentBound: true,
  mailboxBindingReviewed: true,
  accountAuthorityReviewed: true,
};

describe("OSL Mail Stage C gate", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("keeps current mailbox operations off until Stage C is separately reviewed", async () => {
    const { oslMailboxStageCGate } = await loadUi();

    const gate = oslMailboxStageCGate(acceptedReview);

    expect(gate).toMatchObject({
      operationsAllowed: false,
      label: "Coming later",
      reason: "stage-c-coming-later",
    });
    expect(gate.detail).toContain("Mailbox operations stay off");
    expect(gate.detail).toContain("separate mail operations review");
  });

  it("refuses future Stage C without the separate mail operations review", async () => {
    const { oslMailboxStageCGate } = await loadUi();

    const gate = oslMailboxStageCGate(null, reviewedStageC);

    expect(gate).toMatchObject({
      operationsAllowed: false,
      label: "Unavailable",
      reason: "separate-mail-operations-review-required",
    });
  });

  it("refuses future Stage C when consent, mailbox access, or authority is absent", async () => {
    const { oslMailboxStageCGate } = await loadUi();

    for (const missing of ["explicitConsentBound", "mailboxBindingReviewed", "accountAuthorityReviewed"] as const) {
      const gate = oslMailboxStageCGate({ ...acceptedReview, [missing]: false }, reviewedStageC);
      expect(gate, missing).toMatchObject({
        operationsAllowed: false,
        reason: "mailbox-consent-binding-authority-required",
      });
    }
  });

  it("allows only a reviewed Stage C mailbox with consent, mailbox access, and authority", async () => {
    const { oslMailboxStageCGate } = await loadUi();

    const gate = oslMailboxStageCGate(acceptedReview, reviewedStageC);

    expect(gate).toEqual({
      operationsAllowed: true,
      label: "Reviewed",
      reason: null,
      detail: "Full OSL mailbox operations are available only for this reviewed mailbox.",
    });
  });

  it("renders the Inbox OSL Mail card as refused for mailbox operations", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "inbox" });

    const html = __oslHubUiTest.renderWorkspaceContent("inbox");

    expect(html).toContain('data-inbox-osl-surface="mail"');
    expect(html).toContain('data-osl-mailbox-stage-c-gate="stage-c-coming-later"');
    expect(html).toContain('data-mailbox-operations="refused"');
    // t4-h3 made this copy more specific ("...unavailable until its desktop
    // bridge exists"). Pinning the exact sentence made an honest copy change
    // look like a regression, so assert the PROPERTY the card must communicate
    // -- that the surface is unavailable and says when it changes -- rather
    // than one phrasing of it. The data- attributes above already pin the gate.
    expect(html).toMatch(/coming later|unavailable/i);
    expect(html).toMatch(/OSL Mail/);
    expect(html).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?|auto.?retry|retry automatically/i);
  });
});
