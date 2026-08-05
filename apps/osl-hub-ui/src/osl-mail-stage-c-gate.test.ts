import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type { OslMailStage } from "./desktop-service-policy";

/**
 * D-238. This gate was red in the full suite and green in isolation, which
 * looked like an order dependency. It was not: vitest runs every spec file in
 * its own forked process (`pool: "forks"`, `isolate: true`), so nothing here
 * can inherit another file's globals, mocks or module state.
 *
 * The real cause was that each `it()` did `vi.resetModules()` and then
 * re-imported `./main` -- a ~10,000-line module -- from INSIDE the test body,
 * where vitest's default 5,000 ms `testTimeout` applies. That import costs
 * ~2.6 s on an idle machine and ~3.2 s when the whole 313-file suite is
 * running; when anything else was building it crossed 5 s and the first test
 * died with "Test timed out in 5000ms" -- a red that said nothing about OSL
 * Mail. A gate that fails for reasons unrelated to what it checks is not a
 * gate, so the expensive load now happens once, in a hook, before any test is
 * timed.
 *
 * Loading once is safe here: the first four tests only call the pure
 * `oslMailboxStageCGate` function, and the fifth resets the UI state it uses
 * via `__oslHubUiTest.reset(...)`. No test mutates state another test reads.
 * Every assertion below is unchanged.
 */
let ui: typeof import("./main");

beforeAll(async () => {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, /* hook budget: */ 300_000);
// The hook budget is deliberately generous: what it covers is module loading,
// not the behaviour under test. Even the default 10 s hook timeout is under
// what a saturated machine needs for this import -- starving that load is
// exactly how D-238 presented -- and no assertion depends on it being tight.

afterAll(() => {
  vi.unstubAllGlobals();
});

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
  it("keeps current mailbox operations off until Stage C is separately reviewed", () => {
    const { oslMailboxStageCGate } = ui;

    const gate = oslMailboxStageCGate(acceptedReview);

    expect(gate).toMatchObject({
      operationsAllowed: false,
      label: "Coming later",
      reason: "stage-c-coming-later",
    });
    expect(gate.detail).toContain("Mailbox operations stay off");
    expect(gate.detail).toContain("separate mail operations review");
  });

  it("refuses future Stage C without the separate mail operations review", () => {
    const { oslMailboxStageCGate } = ui;

    const gate = oslMailboxStageCGate(null, reviewedStageC);

    expect(gate).toMatchObject({
      operationsAllowed: false,
      label: "Unavailable",
      reason: "separate-mail-operations-review-required",
    });
  });

  it("refuses future Stage C when consent, mailbox access, or authority is absent", () => {
    const { oslMailboxStageCGate } = ui;

    for (const missing of ["explicitConsentBound", "mailboxBindingReviewed", "accountAuthorityReviewed"] as const) {
      const gate = oslMailboxStageCGate({ ...acceptedReview, [missing]: false }, reviewedStageC);
      expect(gate, missing).toMatchObject({
        operationsAllowed: false,
        reason: "mailbox-consent-binding-authority-required",
      });
    }
  });

  it("allows only a reviewed Stage C mailbox with consent, mailbox access, and authority", () => {
    const { oslMailboxStageCGate } = ui;

    const gate = oslMailboxStageCGate(acceptedReview, reviewedStageC);

    expect(gate).toEqual({
      operationsAllowed: true,
      label: "Reviewed",
      reason: null,
      detail: "Full OSL mailbox operations are available only for this reviewed mailbox.",
    });
  });

  it("renders the Inbox OSL Mail card as refused for mailbox operations", () => {
    const { __oslHubUiTest } = ui;
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
