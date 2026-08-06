import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of the single `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of the test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// test reads it synchronously.
let ui: typeof import("./main");

beforeAll(async () => {
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
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

describe("RN wire policy UI", () => {
  it("expose RN wire-in policy toggle in Tauri settings/UI", () => {
    const { readRnWirePolicyRequested, rnWirePolicySettingsMarkup, rnWirePolicyState } = ui;
    const savedOn = new Map([["osl-rn-wire-policy-requested-v1", "true"]]);
    const storage = {
      getItem: vi.fn((key: string) => savedOn.get(key) ?? null),
    };
    const requested = readRnWirePolicyRequested(storage as Pick<Storage, "getItem">);

    const disabled = rnWirePolicyState(requested, false);
    expect(disabled).toEqual({
      requested: true,
      buildEnabled: false,
      effectiveEnabled: false,
      refusal: "build-disabled",
    });
    expect(storage.getItem).toHaveBeenCalledWith("osl-rn-wire-policy-requested-v1");
    const markup = rnWirePolicySettingsMarkup(disabled);
    expect(markup).toContain('id="rn-wire-policy-toggle"');
    expect(markup).toContain("disabled");
    expect(markup).not.toContain("checked");
    expect(markup).toContain("Unavailable in this build");
    expect(markup).not.toContain("<small>Off</small>");

    expect(rnWirePolicyState(true, true)).toMatchObject({ effectiveEnabled: true, refusal: null });
  });
});
