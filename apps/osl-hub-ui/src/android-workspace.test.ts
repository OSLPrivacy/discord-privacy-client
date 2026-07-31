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

describe("Android workspace rendering", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders Android Mobile Workspace as future Pro isolation", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "connections" });

    const html = __oslHubUiTest.renderWorkspaceContent("connections");

    expect(html).toContain('data-android-surface="androidMobileWorkspace"');
    expect(html).toContain("Android Mobile Workspace");
    expect(html).toContain("Coming later · Pro");
    expect(html).toContain('data-workspace-runtime="localVirtualDevice"');
    expect(html).toContain("encrypted local virtual device storage");
    expect(html).toContain("clipboard, files, notifications, camera, microphone, and location start denied");
  });

  it("keeps hosted Android workspace behind separate threat model consent", async () => {
    const { __oslHubUiTest, androidWorkspaceCardMarkup } = await loadUi();
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
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Renders Android Mobile Workspace helper as future Pro isolation", async () => {
    const { androidWorkspaceCardMarkup } = await loadUi();

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

  it("Keeps hosted Android workspace helper behind separate threat model consent", async () => {
    const { androidWorkspaceCardMarkup } = await loadUi();

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
