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

function sendModeButtons(markup: string): string[] {
  return [...markup.matchAll(/data-send-mode="([^"]+)"/gu)].map((match) => match[1]);
}

describe("onboarding send modes", () => {
  it("Implement ordinary send choices without Single Enter", async () => {
    installGlobals();
    const { sendingSetupContent } = await import("./main");

    const markup = sendingSetupContent();

    expect(sendModeButtons(markup)).toEqual(["manual", "clipboard", "double"]);
    expect(markup).toContain("Manual");
    expect(markup).toContain("Clipboard");
    expect(markup).toContain("Double Enter");
    expect(markup).not.toContain('data-send-mode="single"');
    expect(markup).not.toMatch(/Single Enter/iu);
    expect(markup).toMatch(/No mode silently sends/iu);
  });
});
