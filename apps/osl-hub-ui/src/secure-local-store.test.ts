import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { registerSecureLocalStoreTests, SecureLocalStore } from "./secure-local-store";

class MemoryStorage implements Pick<Storage, "getItem" | "removeItem" | "setItem"> {
  readonly items = new Map<string, string>();

  getItem(key: string): string | null {
    return this.items.get(key) ?? null;
  }

  removeItem(key: string): void {
    this.items.delete(key);
  }

  setItem(key: string, value: string): void {
    this.items.set(key, value);
  }
}

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

async function secureStore(storage: MemoryStorage, randomByte = 4): Promise<SecureLocalStore> {
  const rawKey = new Uint8Array(32);
  rawKey.fill(42);
  return new SecureLocalStore({
    storage,
    key: await SecureLocalStore.importRawKey(rawKey),
    randomBytes: (bytes) => {
      bytes.fill(randomByte);
      return bytes;
    },
  });
}

registerSecureLocalStoreTests({ describe, expect, it });

describe("SecureLocalStore integration contracts", () => {
  it("stores migrated values under an envelope without the plaintext value", async () => {
    const storage = new MemoryStorage();
    const store = await secureStore(storage);

    await store.setItem("osl-chat-unread-v1", JSON.stringify({ "person-private": 3 }));

    const entries = [...storage.items.entries()];
    expect(entries).toHaveLength(1);
    expect(entries[0][0]).not.toBe("osl-chat-unread-v1");
    expect(entries[0][1]).not.toContain("person-private");
    expect(JSON.parse(entries[0][1])).toMatchObject({ v: 1, alg: "AES-256-GCM" });
    await expect(store.getItem("osl-chat-unread-v1")).resolves.toBe('{"person-private":3}');
  });

  it("routes all OSL Chat metadata migrations through the async SecureLocalStore hook", () => {
    const main = readRelative("./main.ts");

    expect(main).toContain("type OslChatSecureStore = Pick<SecureLocalStore, \"getItem\" | \"setItem\">");
    expect(main).toContain("secureOrLegacyOslChatPreference(store, storage, oslChatMutedStorageKey)");
    expect(main).toContain("secureOrLegacyOslChatPreference(store, storage, oslChatUnreadStorageKey)");
    expect(main).toMatch(/secureOrLegacyOslChatPreference\(\s*oslChatSecureStore,\s*localStorage,\s*oslChatNotificationStorageKey,\s*\)/u);
    expect(main).toContain("persistSensitiveOslChatJson(oslChatMutedStorageKey");
    expect(main).toContain("persistSensitiveOslChatJson(oslChatUnreadStorageKey");
    expect(main).toContain("persistSensitiveOslChatJson(oslChatNotificationStorageKey");
    expect(main).not.toMatch(/localStorage\.setItem\(\s*oslChat(?:Muted|Unread|Notification)StorageKey/u);
  });

  it("keeps peer_map and membership in the mandatory encrypted-state sweep", () => {
    const peerMap = readRelative("../../../crates/ipc/src/peer_map.rs");
    const membership = readRelative("../../../crates/ipc/src/membership.rs");
    const reload = readRelative("../../../crates/ipc/src/state_reload.rs");
    const commands = readRelative("../../../crates/ipc/src/commands.rs");

    expect(peerMap).toContain("refusing to write plaintext peer_map over encrypted file");
    expect(peerMap).toContain("crate::main_password::maybe_encrypt(body.as_bytes())");
    expect(membership).toContain("refusing to write plaintext membership over encrypted file");
    expect(membership).toContain("crate::main_password::encrypt_at_rest(&body, &key)");
    expect(membership).toContain("fn reload_reencrypts_plaintext_membership_when_key_now_present()");
    expect(reload).toContain('"membership.json"');
    expect(reload).toContain("report.scope_membership_reencrypted = true");
    expect(commands).toContain("deferring membership persist");
  });

  it("does not enable the RN wire-in gate", () => {
    const wireRn = readRelative("../../../crates/ipc/src/wire_rn.rs");
    expect(wireRn).toContain("pub const RN_WIRE_IN_ENABLED: bool = false;");
  });
});
