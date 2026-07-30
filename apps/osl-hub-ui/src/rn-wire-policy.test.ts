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

describe("RN wire policy UI", () => {
  it("expose RN wire-in policy toggle in Tauri settings/UI", async () => {
    installGlobals();
    const { rnWirePolicySettingsMarkup, rnWirePolicyState } = await import("./main");

    const disabled = rnWirePolicyState(true, false);
    expect(disabled).toEqual({
      requested: true,
      buildEnabled: false,
      effectiveEnabled: false,
      refusal: "build-disabled",
    });
    const markup = rnWirePolicySettingsMarkup(disabled);
    expect(markup).toContain('id="rn-wire-policy-toggle"');
    expect(markup).toContain("disabled");
    expect(markup).not.toContain("checked");

    expect(rnWirePolicyState(true, true)).toMatchObject({ effectiveEnabled: true, refusal: null });
  });
});
