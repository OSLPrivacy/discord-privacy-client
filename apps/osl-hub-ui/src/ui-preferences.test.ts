import { afterEach, describe, expect, it, vi } from "vitest";
import { SecureLocalStore } from "./secure-local-store";

const mocks = vi.hoisted(() => ({
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({
  browserLogo: vi.fn(() => ""),
  providerLogo: vi.fn(() => ""),
  serviceLogo: vi.fn(() => ""),
}));

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
  keyBytes.fill(31);
  return new SecureLocalStore({
    storage,
    key: await SecureLocalStore.importRawKey(keyBytes),
    randomBytes: (bytes) => {
      bytes.fill(8);
      return bytes;
    },
  });
}

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

describe("UI preferences", () => {
  it("loadUiPreferences decrypts all three migrated OSL chat preference keys", async () => {
    const legacy = new MemoryStorage();
    legacy.setItem("osl-chat-previews-visible-v1", "true");
    legacy.setItem("osl-chat-muted-people-v1", JSON.stringify(["legacy-person"]));
    legacy.setItem("osl-chat-unread-v1", JSON.stringify({ "legacy-person": 1 }));
    installGlobals(legacy);

    const encrypted = new MemoryStorage();
    const writer = await secureStore(encrypted);
    await writer.setItem("osl-chat-previews-visible-v1", "false");
    await writer.setItem("osl-chat-muted-people-v1", JSON.stringify(["person-a", "person-b"]));
    await writer.setItem("osl-chat-unread-v1", JSON.stringify({ "person-a": 3, "person-d": 9 }));

    const main = await import("./main");
    main.configureOslChatSecureLocalStore(await secureStore(encrypted));
    await main.loadUiPreferences();

    expect(main.oslChatUiPreferenceSnapshot()).toEqual({
      previewsVisible: false,
      mutedPeople: ["person-a", "person-b"],
      unread: [["person-a", 3], ["person-d", 9]],
    });
    expect([...encrypted.values.values()].join("\n")).not.toContain("person-a");
    expect([...encrypted.values.values()].join("\n")).not.toContain("person-d");
  });

  it("loadUiPreferences falls back per migrated key when a secure value is absent or refused", async () => {
    const legacy = new MemoryStorage();
    legacy.setItem("osl-chat-previews-visible-v1", "false");
    legacy.setItem("osl-chat-muted-people-v1", JSON.stringify(["legacy-muted"]));
    legacy.setItem("osl-chat-unread-v1", JSON.stringify({ "legacy-unread": 1 }));
    installGlobals(legacy);

    const main = await import("./main");
    main.configureOslChatSecureLocalStore({
      getItem: async (key: string) => {
        if (key === "osl-chat-previews-visible-v1") return null;
        if (key === "osl-chat-muted-people-v1") throw new Error("decrypt refused");
        if (key === "osl-chat-unread-v1") return JSON.stringify({ "secure-unread": 4 });
        return null;
      },
      setItem: async () => undefined,
    });
    await main.loadUiPreferences();

    expect(main.oslChatUiPreferenceSnapshot()).toEqual({
      previewsVisible: false,
      mutedPeople: ["legacy-muted"],
      unread: [["secure-unread", 4]],
    });
  });
});
