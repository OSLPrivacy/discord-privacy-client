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
// each test either drives the state it renders from through
// `__oslHubUiTest.reset(...)` or calls pure exported helpers, and the stubbed
// `localStorage` is emptied before each test -- which is exactly the state a
// fresh import would have seen.
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

describe("Android workspace rendering", () => {
  it("renders Android Mobile Workspace as future Pro isolation", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ route: "connections" });

    const html = __oslHubUiTest.renderWorkspaceContent("connections");

    expect(html).toContain('data-android-surface="androidMobileWorkspace"');
    expect(html).toContain("Android Mobile Workspace");
    expect(html).toContain("Coming later · Pro");
    expect(html).toContain('data-workspace-runtime="localVirtualDevice"');
    expect(html).toContain("encrypted local virtual device storage");
    expect(html).toContain("clipboard, files, notifications, camera, microphone, and location start denied");
  });

  it("keeps hosted Android workspace behind separate threat model consent", () => {
    const { __oslHubUiTest, androidWorkspaceCardMarkup } = ui;
    __oslHubUiTest.reset({ route: "connections" });

    const html = __oslHubUiTest.renderWorkspaceContent("connections");
    const card = androidWorkspaceCardMarkup();
    const copy = visibleText(card);

    expect(html).toContain('data-hosted-execution="false"');
    expect(html).toContain('data-consent="required"');
    expect(html).toContain("separate mobile workspace threat model review and explicit consent");
    expect(html).toContain("No hosted Android workspace runs from this card");
    expect(copy).toContain("Future Pro isolation");
  });
});

describe("Android workspace destination card", () => {
  it("Renders Android Mobile Workspace helper as future Pro isolation", () => {
    const { androidWorkspaceCardMarkup } = ui;

    const markup = androidWorkspaceCardMarkup();
    const copy = visibleText(markup);

    expect(markup).toContain('data-android-surface="androidMobileWorkspace"');
    expect(markup).toContain('data-hosted-execution="false"');
    expect(copy).toMatch(/Android Mobile Workspace/iu);
    expect(copy).toMatch(/Future Pro isolation/iu);
    expect(copy).toMatch(/Coming later/iu);
    expect(copy).toMatch(/Encrypted local virtual device storage/iu);
    expect(copy).toMatch(/clipboard, files, notifications, camera, microphone, and location start denied/iu);
  });

  it("Keeps hosted Android workspace helper behind separate threat model consent", () => {
    const { androidWorkspaceCardMarkup } = ui;

    const markup = androidWorkspaceCardMarkup();
    const copy = visibleText(markup);

    expect(markup).toContain('data-android-workspace-consent="required"');
    expect(markup).toContain('aria-disabled="true"');
    expect(markup).toContain("<button");
    expect(markup).toContain("disabled");
    expect(copy).toMatch(/separate mobile workspace threat model review and explicit consent/iu);
    expect(markup).toContain('data-android-workspace-consent="required"');
    expect(markup).toContain('aria-disabled="true"');
    expect(copy).toMatch(/Hosted workspace is unavailable here/iu);
    expect(copy).toMatch(/No hosted Android workspace runs from this card/iu);
    expect(copy).not.toMatch(/open workspace|launch Android|enabled by default|hosted workspace ready/iu);
  });
});
