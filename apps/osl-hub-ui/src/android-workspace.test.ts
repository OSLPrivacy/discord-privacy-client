import { beforeEach, describe, expect, it, vi } from "vitest";

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
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
    expect(html).toContain("clipboard, files, notifications, camera, microphone, and location denied by default");
  });

  it("keeps hosted Android workspace behind separate threat model consent", async () => {
    const { __oslHubUiTest, androidWorkspaceCardMarkup } = await loadUi();
    __oslHubUiTest.reset({ route: "connections" });

    const html = __oslHubUiTest.renderWorkspaceContent("connections");
    const card = androidWorkspaceCardMarkup();
    const copy = visibleText(card);

    expect(html).toContain('data-hosted-execution="false"');
    expect(html).toContain('data-consent="required"');
    expect(card).toContain('data-android-workspace-consent="required"');
    expect(card).toContain('aria-disabled="true"');
    expect(copy).toMatch(/Hosted workspace is unavailable here/iu);
    expect(copy).toMatch(/No hosted Android workspace runs from this card/iu);
    expect(copy).not.toMatch(/open workspace|launch Android|enabled by default|hosted workspace ready/iu);
  });
});
