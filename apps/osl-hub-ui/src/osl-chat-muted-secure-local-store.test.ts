import { describe, expect, it, vi } from "vitest";
import { SecureLocalStore } from "./secure-local-store";


// D-251: every `it()` below deliberately re-loads `./main` with its own
// selectors / storage / stubs, so the import CANNOT be hoisted into a single
// `beforeAll` without destroying what the tests check. `src/main.ts` is ~10k
// lines and one load costs ~2.5 s cold, which left almost nothing of vitest's
// default 5,000 ms budget for the behaviour under test: on a busy machine these
// tests died with `Test timed out in 5000ms` before reaching an assertion.
// The budget below covers MODULE LOADING, not the behaviour -- no assertion
// depends on it, and every assertion is unchanged.
const MODULE_RELOAD_BUDGET_MS = 30_000;

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
  readonly removed: string[] = [];
  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void {
    this.removed.push(key);
    this.values.delete(key);
  }
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

async function secureStore(storage: Storage): Promise<SecureLocalStore> {
  const keyBytes = new Uint8Array(32);
  keyBytes.fill(23);
  return new SecureLocalStore({
    storage,
    key: await SecureLocalStore.importRawKey(keyBytes),
    randomBytes: (bytes) => {
      bytes.fill(6);
      return bytes;
    },
  });
}

describe("OSL Chat muted secure storage", () => {
  it("migrate muted-people persistence (oslChatMutedPeople) to SecureLocalSt", async () => {
    const legacy = new MemoryStorage();
    const encrypted = new MemoryStorage();
    legacy.setItem("osl-chat-muted-people-v1", JSON.stringify(["person-a", "", "person-b", "x".repeat(181)]));
    installGlobals(legacy);

    const main = await import("./main");
    const migrated = await main.migrateOslChatMutedPeopleToSecureLocalStore(await secureStore(encrypted), legacy);

    expect([...migrated]).toEqual(["person-a", "person-b"]);
    expect(legacy.getItem("osl-chat-muted-people-v1")).toBeNull();
    expect(legacy.removed).toEqual(["osl-chat-muted-people-v1"]);
    expect([...encrypted.values.values()]).toHaveLength(1);
    expect([...encrypted.values.values()][0]).not.toContain("person-a");
    await expect((await secureStore(encrypted)).getItem("osl-chat-muted-people-v1"))
      .resolves.toBe(JSON.stringify(["person-a", "person-b"]));
  }, MODULE_RELOAD_BUDGET_MS);
});
