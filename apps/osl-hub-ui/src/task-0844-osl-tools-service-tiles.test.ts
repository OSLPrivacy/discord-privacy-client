import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), querySelectorAll: vi.fn(() => []), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => vi.unstubAllGlobals());
beforeEach(() => localStore.clear());

function tile(markup: string, id: string): string {
  return markup.match(new RegExp(`<article[^>]*data-tile-id="${id}"[^>]*>[\\s\\S]*?<\\/article>`, "u"))?.[0] ?? "";
}

describe("TASK 0844 OSL tools and service tiles", () => {
  it("renders named tools, generated service labels, routes, and edit-only hidden tiles", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ route: "home" });
    const home = __oslHubUiTest.renderWorkspaceContent("home");

    for (const [id, label] of [["osl-chats", "OSL Chats"], ["scrub", "Scrub"], ["osl-mail", "OSL Mail"], ["osl-notes", "OSL Notes"]]) {
      expect(tile(home, id)).toContain(label);
    }
    expect(home).toContain('aria-label="OSL tools"');
    expect(home).toContain("Coming later");
    expect(home).not.toContain('class="empty-state"');

    const serviceTile = tile(home, "signal");
    expect(serviceTile).toContain('data-claim-status="comingSoon"');
    expect(serviceTile).toContain("Coming later");

    expect(__oslHubUiTest.openHomeModuleForTest("scrub")).toContain("Privacy");
    __oslHubUiTest.reset({ route: "home" });
    expect(__oslHubUiTest.openHomeModuleForTest("osl-mail")).toContain("OSL Mail");
    __oslHubUiTest.reset({ route: "home" });
    const hidden = __oslHubUiTest.saveHomeTileArrangementForTest(["osl-mail"]);
    expect(hidden).toMatchObject({ saved: true, hiddenIds: ["osl-mail"] });
    expect(tile(__oslHubUiTest.renderWorkspaceContent("home"), "osl-mail")).toBe("");

    console.log("TASK0844_ELEMENTS=OSL Chats|Scrub|Mail|Notes|service tiles");
    console.log("TASK0844_SERVICE_LABEL=Coming later");
    console.log("TASK0844_OPEN_ROUTES=privacy|osl-mail");
    console.log("TASK0844_HIDDEN_TILE=osl-mail");
  });
});
