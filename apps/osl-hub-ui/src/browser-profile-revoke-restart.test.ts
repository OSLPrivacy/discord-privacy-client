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

describe("browser import revoke restart", () => {
  it("Revoke UI wiring + persisted-restart re-read regression (grant/restart", async () => {
    const storage = new MemoryStorage();
    const owner = "owner 1";
    const readyKey = `osl-browser-accounts-ready-v1:${encodeURIComponent(owner)}`;
    const importsKey = `osl-browser-import-sources-v1:${encodeURIComponent(owner)}`;
    storage.setItem(readyKey, "true");
    storage.setItem(importsKey, JSON.stringify(["chrome", "firefox"]));
    installGlobals(storage);
    const { revokeBrowserImportForSource } = await import("./main");

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
