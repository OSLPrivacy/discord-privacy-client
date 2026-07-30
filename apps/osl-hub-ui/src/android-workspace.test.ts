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

describe("Android workspace rendering", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Render Android Mobile Workspace as future Pro isolation", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "connections" });

    const html = __oslHubUiTest.renderWorkspaceContent("connections");

    expect(html).toContain('data-android-surface="androidMobileWorkspace"');
    expect(html).toContain("Android Mobile Workspace");
    expect(html).toContain("Coming later · Pro");
    expect(html).toContain('data-workspace-runtime="localVirtualDevice"');
    expect(html).toContain("encrypted local storage");
    expect(html).toContain("clipboard, files, notifications, camera, microphone, and location denied by default");
  });

  it("Keep hosted Android workspace behind separate threat model consent", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({ route: "connections" });

    const html = __oslHubUiTest.renderWorkspaceContent("connections");

    expect(html).toContain('data-android-surface="androidMobileWorkspace"');
    expect(html).toContain('data-hosted-execution="false"');
    expect(html).toContain('data-consent="required"');
    expect(html).toContain("Hosted workspace is unavailable here");
    expect(html).toContain("separate threat model, explicit consent, and a new audit");
  });
});
