import { describe, expect, it } from "vitest";
import { SecureLocalStore, SecureLocalStoreError, registerSecureLocalStoreTests } from "./secure-local-store";

registerSecureLocalStoreTests({ describe, expect, it });

class MemoryStorage {
  readonly values = new Map<string, string>();
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
}

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
