import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
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

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

function renderedTile(markup: string, tileId: string): string {
  const match = markup.match(new RegExp(`<article[^>]*data-tile-id="${tileId}"[^>]*>[\\s\\S]*?<\\/article>`, "u"));
  expect(match, `${tileId} tile should render`).not.toBeNull();
  return match?.[0] ?? "";
}

describe("OSL Mail Home integration", () => {
  it("renders the first-party Home tile as coming later while keeping the route", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ route: "home" });
    const tile = renderedTile(__oslHubUiTest.renderWorkspaceContent("home"), "osl-mail");

    expect(tile).toMatch(/class="[^"]*\bapp-tile\b[^"]*\bhome-module\b[^"]*\bmodule-unavailable\b/u);
    expect(tile).toContain('data-module-kind="osl-mail"');
    expect(tile).toMatch(/<button\b[^>]*\bdisabled\b/u);
    expect(tile).toContain('aria-label="OSL Mail, coming later"');
    expect(tile).toContain("OSL Mail · Coming later");
    expect(visibleText(tile)).toMatch(/\bOSL Mail\b/u);
    expect(main).toContain('route = "osl-mail"');
    expect(main).toContain('if (route === "osl-mail") return oslMailContent()');
  });

  it("does not call the missing thread-list bridge or coerce failures to an empty inbox", () => {
    expect(main).not.toContain("listOslMailThreads,");
    expect(main).not.toContain("listOslMailThreads()");
    expect(main).not.toContain("retrieveOslMailThread,");
    expect(main).not.toContain("acknowledgeOslMailRetrieval,");
    expect(main).not.toMatch(/listOslMailThreads\(\)\s*\?\?\s*\[\]/u);
    expect(main).toContain("oslMailThreadSyncUnavailable");
  });

  it("provisions only from the claimed signed OSL username", () => {
    expect(main).toContain("provisionOslMail(claimedOslUsername)");
    expect(main).not.toContain("osl-mail-phone");
  });

  it("does not allow external outbound", () => {
    expect(main).toContain('recipient.endsWith("@oslprivacy.com")');
    expect(main).toContain("External outbound mail is unavailable in v1");
  });
});
