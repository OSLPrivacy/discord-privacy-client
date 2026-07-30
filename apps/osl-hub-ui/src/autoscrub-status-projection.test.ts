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

describe("autoscrub status projection", () => {
  it("Status-projection UI renders receipts for a completed run (simple repo", async () => {
    installGlobals();
    const { autoscrubStatusProjectionMarkup } = await import("./main");

    const markup = autoscrubStatusProjectionMarkup({
      phase: "completed",
      receipts: [
        { itemLabel: "Message 1", state: "verified" },
        { itemLabel: "Message 2", state: "notVerified" },
        { itemLabel: "Message 3", state: "held" },
      ],
    });

    expect(markup).toContain('data-phase="completed"');
    expect(markup).toContain('data-cleanup-proof="verified"');
    expect(markup).toContain("Removed");
    expect(markup).toContain("Not verified");
    expect(markup).toContain("Held");
    expect(markup).not.toContain("receipt");
  });
});
