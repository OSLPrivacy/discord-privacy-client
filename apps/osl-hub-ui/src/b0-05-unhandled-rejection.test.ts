import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

class FakeElement {
  className = "";
  innerHTML = "";
  role = "";
  textContent = "";
  readonly classList = { add: vi.fn() };

  constructor(private readonly selectors: Record<string, FakeElement | null> = {}) {}

  addEventListener(): void {
    return;
  }

  querySelector(selector: string): FakeElement | null {
    return this.selectors[selector] ?? null;
  }

  remove(): void {
    return;
  }
}

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key: string) => values.get(key) ?? null,
    key: (index: number) => [...values.keys()][index] ?? null,
    removeItem: (key: string) => { values.delete(key); },
    setItem: (key: string, value: string) => { values.set(key, value); },
  };
}

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of the only `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of the test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// test reads it synchronously. The old `beforeEach(() => { vi.unstubAllGlobals();
// vi.restoreAllMocks(); })` ran BEFORE the (only) test, which would have torn
// down the globals just stubbed in `beforeAll` before the test ever ran; it is
// now an `afterAll` instead.
let append: ReturnType<typeof vi.fn>;
let ui: typeof import("./main");

beforeAll(async () => {
  vi.resetModules();
  mocks.invoke.mockReset();
  const appFrame = new FakeElement();
  const root = new FakeElement({ ".app-frame": appFrame });
  append = vi.fn();
  vi.stubGlobal("document", {
    querySelector: (selector: string) => selector === "#app" ? root : null,
    createElement: () => new FakeElement(),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    body: { append },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout: vi.fn(() => 1),
    clearTimeout: vi.fn(),
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("localStorage", memoryStorage());
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("B0-05 unhandled Tauri command rejection visibility", () => {
  it("surfaces a nonexistent command rejection while still containing background failure", async () => {
    const { __oslHubUiTest } = ui;
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const missingCommand = "b0_05_command_that_does_not_exist";
    const missingCommandRejection = new Error(`unknown command ${missingCommand}`);
    mocks.invoke.mockRejectedValueOnce(missingCommandRejection);

    const rejectionReason = await invoke(missingCommand).catch((reason: unknown) => reason);
    const preventDefault = vi.fn();
    const event = {
      preventDefault,
      reason: rejectionReason,
    } as unknown as PromiseRejectionEvent;
    __oslHubUiTest.handleUnhandledRejection(event);

    expect(mocks.invoke).toHaveBeenCalledWith(missingCommand);
    expect(preventDefault).not.toHaveBeenCalled();
    expect(consoleError).toHaveBeenCalledWith("Unhandled background rejection", missingCommandRejection);
    expect(append).toHaveBeenCalledTimes(1);
    // Strengthened after the B0-05 Adversary FAIL: routing the command name to
    // console.error alone left the only human-visible path a generic toast,
    // which is indistinguishable from a network blip. The toast must NAME the
    // command, or this task has not surfaced anything a person can act on.
    const toast = (append.mock.calls[0]?.[0] as FakeElement | undefined)?.textContent ?? "";
    expect(toast).toContain("That action failed. Nothing changed.");
    expect(toast).toContain(missingCommand);
  });
});
