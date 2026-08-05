import { beforeAll, describe, expect, it, vi } from "vitest";

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

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of the only `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of the test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// test reads it synchronously.
let ui: typeof import("./main");

beforeAll(async () => {
  installGlobals();
  ui = await import("./main");
}, 300_000);

describe("autoscrub status projection", () => {
  it("Status-projection UI renders receipts for a completed run (simple repo", () => {
    const { autoscrubStatusProjectionMarkup } = ui;

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
