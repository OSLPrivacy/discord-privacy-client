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
// each test drives the state it renders from through `__oslHubUiTest.reset(...)`,
// and the stubbed `localStorage` is emptied before each test -- which is
// exactly the state a fresh import would have seen. The `VITE_OSL_ENCLAVE_DEMO_AUDIENCES`
// env stub in the first test is asserting that this build's rendering does NOT
// consult that variable at all, so it makes no difference that it is set after
// the module has already been imported once, in `beforeAll`.
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
  vi.unstubAllEnvs();
});

beforeEach(() => {
  localStore.clear();
});

function circlesCard(inboxHtml: string): string {
  return inboxHtml.match(/<section class="inbox-surface-card enclaves-destination"[\s\S]*?<\/section>\s*<\/section>/u)?.[0]
    ?? inboxHtml.match(/<section class="inbox-surface-card enclaves-destination"[\s\S]*/u)?.[0]
    ?? "";
}

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

// Every person and audience name that the shipped fixture invented. None of
// them is a real contact, so none of them may ever appear on a real account.
const inventedNames = ["Close friends", "Family", "Book club", "Work", "Neighborhood", "Maya", "Theo", "Rina", "Ari", "Sam"];

describe("OSL Enclaves reflects real trust state", () => {
  it("shows nobody on a fresh account with no verified people", () => {
    // A shipping account must remain empty even if an old development-only
    // demo switch is present in its environment.
    vi.stubEnv("VITE_OSL_ENCLAVE_DEMO_AUDIENCES", "1");
    const { __oslHubUiTest } = ui;
    // A brand-new unlocked account: no friends, no services, nothing verified.
    __oslHubUiTest.reset({ route: "inbox", coreReady: true, hubPeople: [], services: [] });

    const inbox = __oslHubUiTest.renderWorkspaceContent("inbox");
    const card = circlesCard(inbox);
    const copy = visibleText(card);

    expect(card).not.toBe("");
    for (const name of inventedNames) {
      expect(copy, `Enclaves must not name "${name}" on an account with no people`).not.toContain(name);
    }
    // Nothing may claim a member count either: "8 people" is as much a trust
    // claim as the names behind it.
    expect(copy).not.toMatch(/\d+\s+people/u);
    expect(card).toContain('data-enclave-audience-count="0"');
    expect(card).not.toContain("data-enclave-audience=");
    expect(card).not.toContain('data-enclave-posting="ready"');
    expect(card).toContain('data-enclave-audiences="none"');
    expect(copy).toMatch(/No Enclave audiences yet/u);
  });

  it("names members only from the audiences it was actually given", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({
      route: "inbox",
      coreReady: true,
      enclaveAudienceRecords: [
        { audienceId: "a".repeat(32), name: "Hiking group", memberCount: 1, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Robin", verified: true }], consentGranted: true, boundToCurrentEnclave: true, postingAuthorized: true },
        { audienceId: "d".repeat(32), name: "Choir", memberCount: 2, membershipVisibility: "count-only", visibleMembers: [], consentGranted: false, boundToCurrentEnclave: true, postingAuthorized: true },
      ],
    });

    const card = circlesCard(__oslHubUiTest.renderWorkspaceContent("inbox"));
    const copy = visibleText(card);

    expect(card).toContain('data-enclave-audience-count="2"');
    expect(copy).toContain("Hiking group");
    expect(copy).toContain("Robin verified");
    expect(copy).toContain("Choir");
    expect(card).toContain('data-enclave-posting="ready"');
    expect(card).toContain('data-enclave-posting="refused"');
    expect(card).not.toContain('data-enclave-audiences="none"');
    // Still nothing invented alongside the real ones.
    for (const name of inventedNames) {
      expect(copy).not.toContain(name);
    }
  });

  it("keeps Home's verified-people count and the Enclaves surface telling the same story", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ route: "home", coreReady: true, hubPeople: [] });

    const home = visibleText(__oslHubUiTest.renderWorkspaceContent("home"));
    const enclaves = visibleText(circlesCard(__oslHubUiTest.renderWorkspaceContent("inbox")));

    expect(home).toContain("Trusted people");
    expect(home).toContain("0 verified");
    // Home said zero. Enclaves may not simultaneously show approved audiences.
    expect(enclaves).toMatch(/No Enclave audiences yet/u);
  });
});
