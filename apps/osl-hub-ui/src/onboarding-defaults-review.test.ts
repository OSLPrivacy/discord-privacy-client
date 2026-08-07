import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { nextOnboardingRoute, previousOnboardingRoute } from "./onboarding-sequence";
import { initialDeleteChoices, onboardingDeleteMarkup } from "./onboarding-delete";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

class FakeElement {
  className = "";
  dataset: Record<string, string> = {};
  role = "";
  textContent = "";
  value = "";
  checked = false;
  innerHTML = "";
  classList = { add: vi.fn() };
  private listeners = new Map<string, Array<(event: { currentTarget: FakeElement; preventDefault: () => void }) => void>>();

  addEventListener(type: string, listener: (event: { currentTarget: FakeElement; preventDefault: () => void }) => void): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  dispatch(type: string): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener({ currentTarget: this, preventDefault: vi.fn() });
    }
  }

  append(): void {}
  focus(): void {}
  querySelector(): null { return null; }
  querySelectorAll(): unknown[] { return []; }
  remove(): void {}
}

function installControlDom(selectors: Record<string, FakeElement>): void {
  vi.stubGlobal("document", {
    querySelector: vi.fn((selector: string) => selectors[selector] ?? null),
    querySelectorAll: vi.fn(() => []),
    createElement: vi.fn(() => new FakeElement()),
    getElementById: vi.fn(() => null),
    documentElement: new FakeElement(),
    body: new FakeElement(),
    activeElement: null,
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
}

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of the one `it()` that needs the live module,
// where vitest's default 5,000 ms `testTimeout` applies, so most of that test's
// budget went on module loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// one test that needs it reads it synchronously. That is safe here and was
// checked, not assumed: it is the only test in this file that touches the
// module, and it does not call `__oslHubUiTest.reset(...)` at all -- it only
// reads pure exported content functions, which is exactly what a fresh import
// would have exposed.
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined });
  vi.stubGlobal("HTMLElement", FakeElement);
  vi.stubGlobal("HTMLInputElement", FakeElement);
  vi.stubGlobal("HTMLTextAreaElement", FakeElement);
  vi.stubGlobal("HTMLSelectElement", FakeElement);
  installControlDom({});
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

describe("review defaults onboarding", () => {
  // 2026-08-06 restyle: the screen moved into its own module, so what is checked
  // is the markup it renders rather than the source that used to produce it.
  const review = onboardingDeleteMarkup(initialDeleteChoices());

  // 2026-08-06 re-split, then restyled. This screen used to echo the six rows
  // the preset screen had just shown. It became the two "what does OSL keep"
  // settings, and is now phrased as "What should OSL delete?" -- same two
  // settings, asked the other way round.
  //
  // THE POINT OF THE REPHRASING, and the thing this test exists to hold: asking
  // what to KEEP put the safe answer on the side where the switches were lit.
  // Asking what to DELETE puts the destructive answer behind a deliberate
  // action, so both defaults are off and nothing is deleted unless someone says
  // so here.
  it("asks what to delete, with both destructive choices off", () => {
    expect(review).toContain("What should OSL delete?");
    expect(review).toContain("Unsent private messages");
    expect(review).toContain("Old messages");
    expect(initialDeleteChoices()).toEqual({ deleteDrafts: false, deleteOldMessages: false });
    expect(review).not.toContain('id="delete-drafts" checked');
    expect(review).not.toContain('id="delete-old-messages" checked');
    // Each row is its own id, so one cannot be flipped as a side effect of the
    // other -- which on a delete screen would destroy something unasked.
    const flipped = onboardingDeleteMarkup({ deleteDrafts: true, deleteOldMessages: false });
    expect(flipped).toContain('id="delete-drafts" checked');
    expect(flipped).not.toContain('id="delete-old-messages" checked');
    expect(review).toContain('id="continue-defaults-review"');
  });

  // Guards two deletions. "A private record of what happened" was removed on the
  // owner's instruction -- OSL must not keep an activity record at all -- and
  // this screen must not go back to echoing the previous screen's six rows.
  it("does not re-add the activity record or re-echo the before-send rows", () => {
    expect(review).not.toMatch(/A private record of what happened|activity (?:record|log|history)/iu);
    expect(review).not.toMatch(/Warn (?:me )?before/u);
    expect(review).not.toContain("Clean attachments");
    expect(review).not.toContain("Review defaults");
    expect(review).not.toContain("What should OSL keep on this device?");
  });

  // Protects the rule, not its old wording: setup starts no destructive
  // automation, and deleting anything still needs a separate review and a
  // confirmation afterwards.
  it("starts destructive automation off and requires later review plus confirmation", () => {
    // The rule survives the rewording twice over: the defaults are off, and the
    // screen still says out loud that nothing goes without a confirmation.
    expect(initialDeleteChoices().deleteOldMessages).toBe(false);
    expect(initialDeleteChoices().deleteDrafts).toBe(false);
    expect(review).toContain("Nothing is deleted without confirmation");
    expect(review).not.toContain("auto-retry");
    expect(review).not.toContain("retry automatically");
    expect(review).not.toContain("Single Enter");
  });

  // Protects the first-run reading level on this screen.
  it("keeps implementation concepts out of first-run copy", () => {
    expect(review).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter/iu);
  });

  // Protects: this screen sits between the explicit Tor choice and send setup,
  // and both neighbours still render, so the route order cannot silently strand it.
  it("is wired between protection presets, the explicit Tor choice, and send setup", () => {
    const branches = { detected: false, install: false };
    const { reviewDefaultsOnboardingContent, sendingSetupContent } = ui;

    expect(nextOnboardingRoute("privacy", branches)).toBe("defaults");
    expect(previousOnboardingRoute("defaults", branches)).toBe("privacy");
    expect(nextOnboardingRoute("defaults", branches)).toBe("tor");
    expect(previousOnboardingRoute("tor", branches)).toBe("defaults");
    expect(nextOnboardingRoute("tor", branches)).toBe("sending");
    expect(previousOnboardingRoute("sending", branches)).toBe("tor");
    expect(nextOnboardingRoute("cover", branches)).toBe("visibility");
    expect(previousOnboardingRoute("visibility", branches)).toBe("cover");
    expect(nextOnboardingRoute("privacy", branches)).toBe("tor");
    expect(previousOnboardingRoute("tor", branches)).toBe("privacy");
    expect(nextOnboardingRoute("tor", branches)).toBe("defaults");
    expect(previousOnboardingRoute("defaults", branches)).toBe("tor");
    expect(nextOnboardingRoute("defaults", branches)).toBe("sending");
    expect(previousOnboardingRoute("sending", branches)).toBe("defaults");
    expect(reviewDefaultsOnboardingContent()).toContain('id="continue-defaults-review"');
    expect(sendingSetupContent()).toContain('data-send-mode="manual"');
    expect(sendingSetupContent()).toContain('id="finish-onboarding"');
  });

  it("blocks Continue when the defaults record is missing, then Back returns to protection level", () => {
    const continueButton = new FakeElement();
    const backButton = new FakeElement();
    installControlDom({
      "#continue-defaults-review": continueButton,
      "#onboarding-back": backButton,
    });

    ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "defaults" });
    ui.__oslHubUiTest.setDeleteChoicesForTest(null);

    expect(ui.reviewDefaultsOnboardingContent()).toContain('id="defaults-record-missing"');
    ui.__oslHubUiTest.bindOnboarding();

    continueButton.dispatch("click");
    const afterContinue = ui.__oslHubUiTest.snapshot().onboardingRoute;
    expect(afterContinue).toBe("defaults");

    backButton.dispatch("click");
    const afterBack = ui.__oslHubUiTest.snapshot().onboardingRoute;
    expect(afterBack).toBe("privacy");

    console.log(`TASK0342_MISSING_DEFAULTS_RECORD=missing`);
    console.log(`TASK0342_CONTINUE_ROUTE_AFTER_MISSING_DEFAULTS=${afterContinue}`);
    console.log(`TASK0342_CONTINUE_BLOCKED=${afterContinue === "defaults"}`);
    console.log(`TASK0342_BACK_ROUTE_AFTER_MISSING_DEFAULTS=${afterBack}`);
    console.log(`TASK0342_PROTECTION_LEVEL_ROUTE=${afterBack}`);
  });
});
