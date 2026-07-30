import { afterEach, describe, expect, it, vi } from "vitest";
import { SecureLocalStore, SecureLocalStoreError, registerSecureLocalStoreTests } from "./secure-local-store";

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
  keyBytes.fill(37);
  return new SecureLocalStore({
    storage,
    key: await SecureLocalStore.importRawKey(keyBytes),
    randomBytes: (bytes) => {
      bytes.fill(9);
      return bytes;
    },
  });
}

registerSecureLocalStoreTests({ describe, expect, it });

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

describe("SecureLocalStore aggregate mandatory encryption", () => {
  it("aggregates mandatory-encryption refusal with UI secure local-store migration", async () => {
    const rawKey = new Uint8Array(32);
    rawKey.fill(93);
    const encryptedStorage = new MemoryStorage();
    const store = new SecureLocalStore({
      storage: encryptedStorage,
      key: await SecureLocalStore.importRawKey(rawKey),
      randomBytes: (bytes) => {
        bytes.fill(11);
        return bytes;
      },
    });
    const sensitivePreference = JSON.stringify({ "person-a": 3, "person-b": 1 });

    await store.setItem("osl-chat-unread-v1", sensitivePreference);

    const [[storageKey, rawEnvelope]] = [...encryptedStorage.values.entries()];
    expect(storageKey).not.toBe("osl-chat-unread-v1");
    expect(rawEnvelope).not.toContain("person-a");
    expect(rawEnvelope).not.toContain(sensitivePreference);
    expect(JSON.parse(rawEnvelope)).toMatchObject({ v: 1, alg: "AES-256-GCM" });
    await expect(store.getItem("osl-chat-unread-v1")).resolves.toBe(sensitivePreference);

    await store.setItem("osl-chat-muted-people-v1", JSON.stringify(["person-c"]));
    const unreadEnvelope = encryptedStorage.values.get(storageKey);
    const mutedStorageKey = [...encryptedStorage.values.keys()].find((key) => key !== storageKey);
    expect(mutedStorageKey).toBeTruthy();
    encryptedStorage.values.set(mutedStorageKey as string, unreadEnvelope as string);
    await expect(store.getItem("osl-chat-muted-people-v1")).rejects.toThrow(SecureLocalStoreError);

    await expect(SecureLocalStore.importRawKey(new Uint8Array(31))).rejects.toThrow(SecureLocalStoreError);
    await expect(
      SecureLocalStore.importRawKey(rawKey, null as unknown as SubtleCrypto),
    ).rejects.toThrow(SecureLocalStoreError);

    const refusedStorage = new MemoryStorage();
    const refusingCrypto = {
      encrypt: async () => {
        throw new Error("mandatory encryption unavailable");
      },
    } as unknown as SubtleCrypto;
    const refusedStore = new SecureLocalStore({
      storage: refusedStorage,
      key: {} as CryptoKey,
      subtle: refusingCrypto,
      randomBytes: (bytes) => {
        bytes.fill(12);
        return bytes;
      },
    });

    await expect(refusedStore.setItem("osl-chat-unread-v1", sensitivePreference))
      .rejects.toThrow(SecureLocalStoreError);
    expect([...refusedStorage.values.values()]).toEqual([]);
  });
});

describe("OSL chat notification secure storage", () => {
  it("migrate persistOslChatNotifications to SecureLocalStore", async () => {
    const legacy = new MemoryStorage();
    const secureWrites: Array<readonly [string, string]> = [];
    installGlobals(legacy);

    const main = await import("./main");
    main.configureOslChatSecureLocalStore({
      getItem: async () => null,
      setItem: async (key, value) => {
        secureWrites.push([key, value]);
      },
    });
    main.__oslHubUiTest.reset({
      appNotifications: [
        { id: "chat-1", title: "OSL Chat", detail: "New encrypted message", createdAt: "Now" },
        { id: "security-1", title: "Safety", detail: "Friend key changed", createdAt: "Now" },
      ],
    });

    main.__oslHubUiTest.persistOslChatNotifications();

    expect(secureWrites).toEqual([[
      "osl-chat-notifications-v1",
      JSON.stringify([{ id: "chat-1", title: "OSL Chat", detail: "New encrypted message", createdAt: "Now" }]),
    ]]);
    expect(legacy.getItem("osl-chat-notifications-v1")).toBeNull();

    const migrationLegacy = new MemoryStorage();
    const encrypted = new MemoryStorage();
    migrationLegacy.setItem("osl-chat-notifications-v1", JSON.stringify([
      { id: "chat-2", title: "OSL Chat", detail: "New encrypted message", createdAt: "Today" },
      { id: "ignored", title: "Other", detail: "Friend key changed", createdAt: "Today" },
      { id: "bad".repeat(40), title: "OSL Chat", detail: "New encrypted message", createdAt: "Today" },
    ]));

    const migrated = await main.migrateOslChatNotificationsToSecureLocalStore(await secureStore(encrypted), migrationLegacy);

    expect(migrated).toEqual([{ id: "chat-2", title: "OSL Chat", detail: "New encrypted message", createdAt: "Today" }]);
    expect(migrationLegacy.getItem("osl-chat-notifications-v1")).toBeNull();
    expect(migrationLegacy.removed).toEqual(["osl-chat-notifications-v1"]);
    expect([...encrypted.values.values()]).toHaveLength(1);
    expect([...encrypted.values.values()][0]).not.toContain("chat-2");
    expect([...encrypted.values.values()][0]).not.toContain("New encrypted message");
    await expect((await secureStore(encrypted)).getItem("osl-chat-notifications-v1"))
      .resolves.toBe(JSON.stringify([{ id: "chat-2", title: "OSL Chat", detail: "New encrypted message", createdAt: "Today" }]));
  });
});
