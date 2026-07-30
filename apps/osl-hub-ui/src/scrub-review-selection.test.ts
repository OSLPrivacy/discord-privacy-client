import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

function installGlobals(): void {
  vi.stubGlobal("localStorage", { getItem: vi.fn(() => null), setItem: vi.fn(), removeItem: vi.fn() });
  vi.stubGlobal("document", {
    querySelector: vi.fn(() => null),
    createElement: vi.fn(() => ({})),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
  });
}

describe("scrub review selection", () => {
  it("toggleScrubReviewSelection lets the owner edit the selected list befor", async () => {
    installGlobals();
    const { toggleScrubReviewSelection } = await import("./main");

    const initial = new Set([0, 1, 99]);
    const removed = toggleScrubReviewSelection(initial, 1, false, 3);
    expect([...removed]).toEqual([0]);

    const added = toggleScrubReviewSelection(removed, 2, true, 3);
    expect([...added]).toEqual([0, 2]);
    expect(toggleScrubReviewSelection(added, 7, true, 3)).toEqual(added);
  });
});
