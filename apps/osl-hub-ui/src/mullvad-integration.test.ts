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

describe("Mullvad integration", () => {
  it("Render Mullvad card without VPN content privacy claims", async () => {
    installGlobals();
    const { mullvadConnectionCardMarkup } = await import("./main");

    const markup = mullvadConnectionCardMarkup({
      availability: "installed",
      integrationState: "availableToOpen",
      privacyScope: "networkOnly",
      connectionState: "notObserved",
    });
    const copy = visibleText(markup);

    expect(markup).toContain('data-connection-kind="mullvad"');
    expect(markup).toContain('data-privacy-scope="networkOnly"');
    expect(copy).toMatch(/Network privacy only/iu);
    expect(copy).toMatch(/separate network tool/iu);
    expect(copy).toMatch(/does not read its account state, connection state, or app content/iu);
    expect(copy).not.toMatch(/message content private|private messages|end-to-end encrypted|anonymous browsing|hides your content|OSL protects Mullvad content/iu);
  });
});
