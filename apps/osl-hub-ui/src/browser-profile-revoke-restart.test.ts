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

class MemoryStorage implements Storage {
  readonly values = new Map<string, string>();
  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

function installGlobals(localStorage: Storage): void {
  vi.stubGlobal("localStorage", localStorage);
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
// used to do that inside the body of every `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of each test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// tests read it synchronously. That is safe here and was checked, not assumed:
// this file has exactly one test, and the storage it exercises is a local
// `MemoryStorage` instance built inside that test and passed explicitly into
// `revokeBrowserImportForSource`, not the globally stubbed `localStorage` --
// so there is no cross-test state for the hoist to disturb.
let ui: typeof import("./main");

beforeAll(async () => {
  installGlobals(new MemoryStorage());
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

describe("browser import revoke restart", () => {
  it("Revoke UI wiring + persisted-restart re-read regression (grant/restart", () => {
    const { revokeBrowserImportForSource } = ui;
    const storage = new MemoryStorage();
    const owner = "owner 1";
    const readyKey = `osl-browser-accounts-ready-v1:${encodeURIComponent(owner)}`;
    const importsKey = `osl-browser-import-sources-v1:${encodeURIComponent(owner)}`;
    storage.setItem(readyKey, "true");
    storage.setItem(importsKey, JSON.stringify(["chrome", "firefox"]));

    const afterFirefox = revokeBrowserImportForSource(storage, owner, "firefox");
    expect([...afterFirefox.completed]).toEqual(["chrome"]);
    expect(afterFirefox.ready).toBe(true);
    expect(JSON.parse(storage.getItem(importsKey) ?? "[]")).toEqual(["chrome"]);

    const afterChrome = revokeBrowserImportForSource(storage, owner, "chrome");
    expect([...afterChrome.completed]).toEqual([]);
    expect(afterChrome.ready).toBe(false);
    expect(storage.getItem(readyKey)).toBeNull();
    expect(storage.getItem(importsKey)).toBeNull();
  });
});
