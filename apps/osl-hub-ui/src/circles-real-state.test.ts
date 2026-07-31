import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), querySelectorAll: vi.fn(() => []), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

function circlesCard(inboxHtml: string): string {
  return inboxHtml.match(/<section class="inbox-surface-card circles-destination"[\s\S]*?<\/section>\s*<\/section>/u)?.[0]
    ?? inboxHtml.match(/<section class="inbox-surface-card circles-destination"[\s\S]*/u)?.[0]
    ?? "";
}

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

// Every person and audience name that the shipped fixture invented. None of
// them is a real contact, so none of them may ever appear on a real account.
const inventedNames = ["Close friends", "Family", "Book club", "Work", "Neighborhood", "Maya", "Theo", "Rina", "Ari", "Sam"];

describe("OSL Circles reflects real trust state", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows nobody on a fresh account with no verified people", async () => {
    const { __oslHubUiTest } = await loadUi();
    // A brand-new unlocked account: no friends, no services, nothing verified.
    __oslHubUiTest.reset({ route: "inbox", coreReady: true, hubPeople: [], services: [] });

    const inbox = __oslHubUiTest.renderWorkspaceContent("inbox");
    const card = circlesCard(inbox);
    const copy = visibleText(card);

    expect(card).not.toBe("");
    for (const name of inventedNames) {
      expect(copy, `Circles must not name "${name}" on an account with no people`).not.toContain(name);
    }
    // Nothing may claim a member count either: "8 people" is as much a trust
    // claim as the names behind it.
    expect(copy).not.toMatch(/\d+\s+people/u);
    expect(card).toContain('data-circle-audience-count="0"');
    expect(card).not.toContain("data-circle-audience=");
    expect(card).not.toContain('data-circle-posting="ready"');
    expect(card).toContain('data-circle-audiences="none"');
    expect(copy).toMatch(/No Circle audiences yet/u);
  });

  it("names members only from the audiences it was actually given", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({
      route: "inbox",
      coreReady: true,
      circleAudienceRecords: [
        { audienceId: "a".repeat(32), name: "Hiking group", memberCount: 1, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Robin", verified: true }], consentGranted: true, boundToCurrentCircle: true, postingAuthorized: true },
        { audienceId: "d".repeat(32), name: "Choir", memberCount: 2, membershipVisibility: "count-only", visibleMembers: [], consentGranted: false, boundToCurrentCircle: true, postingAuthorized: true },
      ],
    });

    const card = circlesCard(__oslHubUiTest.renderWorkspaceContent("inbox"));
    const copy = visibleText(card);

    expect(card).toContain('data-circle-audience-count="2"');
    expect(copy).toContain("Hiking group");
    expect(copy).toContain("Robin verified");
    expect(copy).toContain("Choir");
    expect(card).toContain('data-circle-posting="ready"');
    expect(card).toContain('data-circle-posting="refused"');
    expect(card).not.toContain('data-circle-audiences="none"');
    // Still nothing invented alongside the real ones.
    for (const name of inventedNames) {
      expect(copy).not.toContain(name);
    }
  });

  it("keeps Home's verified-people count and the Circles surface telling the same story", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "home", coreReady: true, hubPeople: [] });

    const home = visibleText(__oslHubUiTest.renderWorkspaceContent("home"));
    const circles = visibleText(circlesCard(__oslHubUiTest.renderWorkspaceContent("inbox")));

    expect(home).toContain("Trusted people");
    expect(home).toContain("0 verified");
    // Home said zero. Circles may not simultaneously show approved audiences.
    expect(circles).toMatch(/No Circle audiences yet/u);
  });
});
