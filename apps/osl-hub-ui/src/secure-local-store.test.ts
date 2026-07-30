import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { SecureLocalStore, secureLocalStorePrefix } from "./secure-local-store";

class MemoryStorage implements Pick<Storage, "getItem" | "removeItem" | "setItem"> {
  private readonly items = new Map<string, string>();

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

describe("SecureLocalStore", () => {
  it("stores migrated values under an envelope without the plaintext value", () => {
    const storage = new MemoryStorage();
    const store = new SecureLocalStore(storage);

    store.setItem("osl-chat-unread", JSON.stringify({ "person-private": 3 }));

    const raw = storage.getItem(`${secureLocalStorePrefix}osl-chat-unread`);
    expect(raw).not.toBeNull();
    expect(raw).not.toContain("person-private");
    expect(store.getItem("osl-chat-unread")).toBe('{"person-private":3}');
  });

  it("falls back from legacy plaintext once and removes the old key after parsing", () => {
    const storage = new MemoryStorage();
    const store = new SecureLocalStore(storage);
    storage.setItem("osl-chat-muted-people-v1", JSON.stringify(["friend-a"]));

    const parsed = store.migrateLegacyItem("osl-chat-muted-people", "osl-chat-muted-people-v1", (raw) => {
      const value = JSON.parse(raw) as unknown;
      return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : null;
    });

    expect(parsed).toEqual(["friend-a"]);
    expect(storage.getItem("osl-chat-muted-people-v1")).toBeNull();
    expect(storage.getItem(`${secureLocalStorePrefix}osl-chat-muted-people`)).not.toContain("friend-a");
  });

  it("routes all OSL Chat metadata migrations through SecureLocalStore", () => {
    const main = readRelative("./main.ts");

    expect(main).toContain('import { SecureLocalStore } from "./secure-local-store"');
    expect(main).toContain("secureLocalStore.migrateLegacyItem(secureOslChatMutedKey, oslChatMutedStorageKey, parseOslChatMutedPeople)");
    expect(main).toContain("secureLocalStore.migrateLegacyItem(secureOslChatUnreadKey, oslChatUnreadStorageKey, parseOslChatUnread)");
    expect(main).toContain("secureLocalStore.migrateLegacyItem(secureOslChatNotificationKey, oslChatNotificationStorageKey, parseOslChatNotifications)");
    expect(main).toContain("secureLocalStore.setItem(secureOslChatMutedKey");
    expect(main).toContain("secureLocalStore.setItem(secureOslChatUnreadKey");
    expect(main).toContain("secureLocalStore.setItem(secureOslChatNotificationKey");
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
    expect(membership).toContain("crate::main_password::maybe_encrypt(&body)");
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
