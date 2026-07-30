import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

function installGlobals(): void {
  vi.stubGlobal("localStorage", { getItem: vi.fn(() => null), setItem: vi.fn(), removeItem: vi.fn() });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
}

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("Android workspace destination card", () => {
  it("Render Android Mobile Workspace as future Pro isolation", async () => {
    installGlobals();
    const { androidWorkspaceCardMarkup } = await import("./main");

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

  it("Keep hosted Android workspace behind separate threat model consent", async () => {
    installGlobals();
    const { androidWorkspaceCardMarkup } = await import("./main");

    const markup = androidWorkspaceCardMarkup();
    const copy = visibleText(markup);

    expect(markup).toContain('data-android-workspace-consent="required"');
    expect(markup).toContain('aria-disabled="true"');
    expect(markup).toContain("<button");
    expect(markup).toContain("disabled");
    expect(copy).toMatch(/separate mobile workspace threat model review and explicit consent/iu);
    expect(copy).toMatch(/No hosted Android workspace runs from this card/iu);
    expect(copy).not.toMatch(/open workspace|launch Android|enabled by default|hosted workspace ready/iu);
  });
});
