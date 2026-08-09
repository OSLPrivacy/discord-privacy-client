import { beforeAll, afterAll, beforeEach, describe, expect, it, vi } from "vitest";

// The Scrub screen must be REACHABLE. The whole choose -> scan -> review
// wizard (scrub-route.ts, consent gate, review list, AutoScrub status) was
// fully built, but "scrub" was missing from the `Route` union and the Home
// launcher's Scrub tile routed to the generic Privacy page instead -- so
// nobody could ever open the finished screen. These tests fail if either
// regression comes back: the tile must open the "scrub" route, and that route
// must render the built wizard rather than throwing into the error boundary.

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
  emitTo: vi.fn(),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  window: {
    isFullscreen: vi.fn(() => Promise.resolve(false)),
    setFullscreen: vi.fn(() => Promise.resolve()),
    onResized: vi.fn(() => Promise.resolve(() => undefined)),
    isMaximized: vi.fn(() => Promise.resolve(false)),
    minimize: vi.fn(() => Promise.resolve()),
    toggleMaximize: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
    setFocus: vi.fn(() => Promise.resolve()),
  },
}));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => mocks.window }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
  providerLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
  serviceLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
}));

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
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
  mocks.invoke.mockReset();
  mocks.isTauriRuntime.mockReturnValue(true);
  ui.__oslHubUiTest.reset({});
});

describe("the Home Scrub tile", () => {
  it('opens the "scrub" route, not the generic Privacy page', () => {
    // The tile once routed to "privacy", which never mounts the wizard.
    expect(ui.__oslHubUiTest.openHomeModuleForTest("scrub")).toBe("scrub");
  });
});

describe("the scrub route", () => {
  it("renders the canonical discovery console without crashing", () => {
    // A throw here fails the test directly; the boundary marker below catches
    // the committed-render path swallowing a crash into the recovery view.
    const markup = ui.__oslHubUiTest.renderWorkspaceContent("scrub");
    // The destination mounts the canonical two-column discovery screen
    // (scrub-discovery-screen.ts): accounts + mode on the left, the streaming
    // console on the right. Discovery deletes nothing and carries no consent
    // gate; the checkbox consent page belongs to the AutoScrub (Pro) path.
    expect(markup).toContain('id="scrub-discovery"');
    expect(markup).toContain("Find what of yours is already out there");
    expect(markup).toContain("CONSOLE");
    expect(markup).toContain("Never logs in for you · never reads saved passwords");
    expect(markup).not.toContain('class="scrub-consent-gate"');
    // A crashed screen must never pass as a rendered one.
    expect(markup).not.toContain("data-osl-render-recovery");
    expect(markup).not.toContain("OSL paused this view");
  });

  it("renders inside the full workspace shell without the error boundary", () => {
    const shell = ui.__oslHubUiTest.renderRouteShell("scrub");
    expect(shell).toContain("scrub-destination");
    expect(shell).not.toContain("data-osl-render-recovery");
  });
});
