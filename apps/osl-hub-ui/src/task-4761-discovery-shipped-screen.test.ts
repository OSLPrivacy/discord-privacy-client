import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * TASK 4761, rendered through the shipped renderer rather than the block on its
 * own. `src/task-4761-discovery-screen.test.ts` checks the block and the Strip
 * row as units; this file proves the same markup is what Settings and the
 * Privacy screen actually put on screen, so a block nobody routed to could not
 * pass for a shipped screen.
 *
 * The import-once hook and the stubbed globals are the pattern already used by
 * `src/ia-settings-placement.test.ts`: `src/main.ts` is ~11k lines and importing
 * it inside an `it()` spends most of the default 5s budget on module loading.
 */
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

describe("TASK 4761 — the shipped Scrub discovery screen", () => {
  it("mounts the canonical two-column Discovery console on the Scrub route", () => {
    const markup = ui.__oslHubUiTest.renderWorkspaceContent("scrub");

    expect(markup).toContain('id="scrub-discovery"');
    expect(markup).toContain("Find what of yours is already out there");
    expect(markup).toContain("ACCOUNTS");
    expect(markup).toContain("Discovery");
    expect(markup).toContain("AutoScrub");
    expect(markup).toContain("CONSOLE");
    expect(markup).toContain("Run discovery");
  });

  it("states the shipped discovery boundary instead of claiming deletion", () => {
    const markup = ui.__oslHubUiTest.renderWorkspaceContent("scrub");

    expect(markup).toContain("Never logs in for you · never reads saved passwords");
    expect(markup).toContain("Discovery + deletion, reviewed batches");
    expect(markup).toContain("AutoScrub is a Pro feature · discovery stays free");
  });
});
