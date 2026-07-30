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

describe("Activity destination", () => {
  it("Implement Activity as the proof destination", async () => {
    installGlobals();
    const { activityDestinationContent, fixedIaRoutePreview } = await import("./main");

    const route = fixedIaRoutePreview().find((target) => target.destination === "activity");
    const markup = activityDestinationContent();
    const copy = visibleText(markup);

    expect(route).toMatchObject({ route: "activity", settingsSection: null });
    expect(markup).toContain('class="content-viewport activity-destination"');
    expect(markup).toContain('aria-label="Recent OSL activity"');
    expect(markup).toContain('id="activity-attention-review"');
    expect(copy).toMatch(/Local proof of what OSL actually did/iu);
    expect(copy).toMatch(/what it refused/iu);
    expect(copy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/iu);
  });
});
