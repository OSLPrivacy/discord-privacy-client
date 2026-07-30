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

describe("public post guard", () => {
  it("Render encrypted-audience carrier preview for public platforms", async () => {
    installGlobals();
    const { publicPostGuardCarrierPreviewMarkup } = await import("./main");

    const markup = publicPostGuardCarrierPreviewMarkup("X");
    const copy = visibleText(markup);

    expect(markup).toContain('data-public-post-guard="encrypted-audience-carrier"');
    expect(markup).toContain('data-carrier-part="public"');
    expect(markup).toContain('data-carrier-part="protected-audience"');
    expect(copy).toMatch(/Encrypted-audience carrier preview/iu);
    expect(copy).toMatch(/public carrier text separately from the protected audience preview/iu);
    expect(copy).toMatch(/If audience proof is missing or changes, OSL refuses/iu);
    expect(copy).not.toMatch(/public .*end-to-end encrypted|ordinary external .*encrypted|available to everyone|global feed ready/iu);
  });
});
