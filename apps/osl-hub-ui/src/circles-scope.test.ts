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
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("Enclaves scope", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders available Enclaves with audiences that can post", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({
      route: "inbox",
      circleAudienceRecords: [
        { audienceId: "a".repeat(32), name: "Hiking group", memberCount: 1, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Robin", verified: true }], consentGranted: true, boundToCurrentCircle: true, postingAuthorized: true },
        { audienceId: "d".repeat(32), name: "Choir", memberCount: 2, membershipVisibility: "count-only", visibleMembers: [], consentGranted: false, boundToCurrentCircle: true, postingAuthorized: true },
      ],
    });

    const inboxHtml = __oslHubUiTest.renderWorkspaceContent("inbox");
    const enclaves = inboxHtml.match(/<section class="inbox-surface-card circles-destination"[\s\S]*?<\/section>/u)?.[0] ?? "";
    const enclaveCopy = visibleText(enclaves);

    expect(enclaves).not.toBe("");
    expect(enclaves).toContain('data-circle-state="available"');
    expect(enclaves).toContain('data-circle-feeds="private-audiences"');
    expect(enclaveCopy).toMatch(/OSL Enclaves/iu);
    expect(enclaveCopy).toMatch(/Posts and comments are encrypted for the selected audience/iu);
    expect(enclaves).toContain('data-circle-posting="ready"');
    expect(enclaves).toContain('data-circle-posting="refused"');
  });

  it("renders the declared available Enclaves state and live audience cards", async () => {
    const { firstPartyOslSurfaceContracts } = await import("./osl-chats-view");
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({
      circleAudienceRecords: [{ audienceId: "a".repeat(32), name: "Hiking group", memberCount: 1, membershipVisibility: "visible", visibleMembers: [], consentGranted: true, boundToCurrentCircle: true, postingAuthorized: true }],
    });

    const circle = firstPartyOslSurfaceContracts().find((surface) => surface.id === "osl-circles");
    const inboxHtml = __oslHubUiTest.renderWorkspaceContent("inbox");

    expect(circle?.state).toBe("available");
    expect(inboxHtml).toContain('data-inbox-osl-surface="circles"');
    expect(inboxHtml).toContain('data-circle-state="available"');
    expect(inboxHtml).toContain("OSL Enclaves");
    expect(inboxHtml).toContain('data-circle-posting="ready"');
    expect(inboxHtml).toContain("Posts and comments are encrypted for the selected audience.");
  });
});
