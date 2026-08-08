import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of every `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of each test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// tests read it synchronously. That is safe here and was checked, not assumed:
// each test drives the state it renders from through `__oslHubUiTest.reset(...)`
// or calls pure exported helpers, and the stubbed `localStorage` is emptied
// before each test -- which is exactly the state a fresh import would have seen.
const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  localStore.clear();
});

function pageText(markup: string): string {
  return markup.replace(/<[^>]+>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("Privacy primary action", () => {
  it("routes Privacy primary action to protection review state", () => {
    const { __oslHubUiTest, privacyPrimaryAction } = ui;
    __oslHubUiTest.reset({ route: "privacy" });

    expect(__oslHubUiTest.renderWorkspaceContent("privacy")).not.toContain("data-privacy-protection-review");

    privacyPrimaryAction();

    expect(__oslHubUiTest.snapshot()).toMatchObject({
      route: "privacy",
      privacyProtectionReviewOpen: true,
    });
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain("data-privacy-protection-review");
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain("Review or change protection");
  });

  it("returns Privacy primary action plan metadata", () => {
    const { privacyDestinationContent, privacyPrimaryActionPlan } = ui;

    const plan = privacyPrimaryActionPlan();
    const markup = privacyDestinationContent();

    expect(plan).toEqual({ route: "privacy", reviewTarget: "protection-review", label: "Review protection" });
    expect(markup).toContain('data-privacy-primary-action');
    expect(markup).toContain('data-route="privacy"');
    expect(markup).toContain('data-review-target="protection-review"');
    expect(markup).toContain('id="privacy-protection-review"');
    expect(markup).toContain("Global policy");
  });
});

// TASK 1540 checked that the Privacy page carries all four disclosures; TASK
// 1541 is the fix side of that pair, so the disclosure sentences below are the
// plain-language wording the page now ships, and the card-details sentence is
// pinned by an exact occurrence count rather than a `toContain`: a page that
// said it twice (once per section, say) would still read as "present" to a
// containment check while telling the reader the same thing twice.
const REQUIRED_DISCLOSURES = [
  "Scrub reads only the files, exports, and account views you choose for review. It does not read your other apps or accounts.",
  "Result text is kept only in the encrypted Scrub index on this device, and it is removed when you clear results or cancel the import.",
  "OSL does not store card details. You type your card on the payment company's own checkout page, so it never reaches OSL.",
  "The payment company may receive checkout, billing, fraud, tax, and support data it needs to take the payment.",
];

const CARD_DETAILS_SENTENCE = "OSL does not store card details";

function countOccurrences(haystack: string, needle: string): number {
  let count = 0;
  let index = haystack.indexOf(needle);
  while (index !== -1) {
    count += 1;
    index = haystack.indexOf(needle, index + needle.length);
  }
  return count;
}

describe("Privacy page disclosures", () => {
  it("finds all four required disclosures in rendered Privacy page text", () => {
    const renderedText = pageText(ui.privacyDestinationContent());
    const found = REQUIRED_DISCLOSURES.filter((disclosure) => renderedText.includes(disclosure));

    console.info(`Privacy disclosure count: ${found.length}`);
    for (const disclosure of found) {
      console.info(`Privacy disclosure found: ${disclosure}`);
    }

    expect(found).toEqual(REQUIRED_DISCLOSURES);
    expect(found).toHaveLength(4);
  });

  it("states the card-details sentence exactly once", () => {
    const renderedText = pageText(ui.privacyDestinationContent());
    const occurrences = countOccurrences(renderedText, CARD_DETAILS_SENTENCE);

    console.info(`Privacy "${CARD_DETAILS_SENTENCE}" occurrences: ${occurrences}`);

    expect(occurrences).toBe(1);
  });
});
