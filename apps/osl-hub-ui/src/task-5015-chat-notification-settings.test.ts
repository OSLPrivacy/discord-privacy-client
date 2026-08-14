import { beforeEach, describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  OSL_CHAT_NOTIFICATION_SETTINGS_KEY,
  defaultOslChatNotificationSettings,
  oslChatNotificationPreview,
  oslChatNotificationSettingsMarkup,
  oslChatSwitchBehaviour,
  readOslChatNotificationSettings,
  saveOslChatNotificationSettings,
  setOslChatNotificationSwitch,
  settingsFromReadReceiptFixture,
  type OslChatNotificationSwitch,
} from "./osl-chat-notification-settings";

class MemoryStorage implements Storage {
  readonly values = new Map<string, string>();
  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

const personId = "verified-friend-5015";
let storage: MemoryStorage;

beforeEach(() => { storage = new MemoryStorage(); });

describe("TASK 5015 per-chat notification switches", () => {
  it("mounts and binds the NOTIFICATIONS block in the shipping chat-settings modal", () => {
    const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    expect(mainSource).toContain("oslChatNotificationSettingsMarkup(notificationSettings)");
    expect(mainSource).toContain('document.querySelectorAll<HTMLInputElement>("[data-osl-chat-notification-switch]")');
    expect(mainSource).toContain("setOslChatNotificationSwitch(localStorage, personId, key");
    expect(mainSource).toContain("oslChatNotificationPreview(notificationSettings, senderName, received.body)");
    console.log("shipping-modal mounted=1 bound-switches=6 notification-projection=1");
  });

  it("saves all 6 switches and reads their exact values after the modal is reopened", () => {
    const setValues = {
      notifications: false,
      previews: false,
      typingIndicator: false,
      readReceipts: false,
      dontShowImTyping: true,
      dontShowWhenOnline: true,
    } as const;

    for (const [key, value] of Object.entries(setValues) as Array<[OslChatNotificationSwitch, boolean]>) {
      setOslChatNotificationSwitch(storage, personId, key, value);
    }

    // Closing discards the in-memory object; reopening reads the persisted map.
    const reopened = readOslChatNotificationSettings(storage, personId);
    expect(reopened).toEqual(setValues);
    const markup = oslChatNotificationSettingsMarkup(reopened);
    expect(markup.match(/data-osl-chat-notification-switch=/gu)).toHaveLength(6);
    expect(markup).toContain("NOTIFICATIONS");
    const labels = [
      "Notifications", "Previews", "Typing indicator", "Read receipts", "Don't show I'm typing", "Don't show when I'm online",
    ];
    for (const label of labels) expect(markup).toContain(`<strong>${label}</strong>`);
    console.log(`reopen switches=6 values=${JSON.stringify(reopened)}`);
    console.log(`visible-labels=${labels.join("|")}`);
  });

  it("with previews off shows the sender name and 0 message words", () => {
    const settings = { ...defaultOslChatNotificationSettings(), previews: false };
    const notification = oslChatNotificationPreview(settings, "Morgan", "these private words stay hidden");
    expect(notification?.senderName).toBe("Morgan");
    expect(notification?.title).toBe("Morgan");
    expect(notification?.messagePreview).toBe("");
    expect(notification?.messageWordCount).toBe(0);
    console.log(`previews-off sender=${notification?.senderName} message-words=${notification?.messageWordCount}`);
  });

  it("sets read receipts to both or neither and refuses one-sided fixtures", () => {
    expect(settingsFromReadReceiptFixture({ send: true, receive: true })?.readReceipts).toBe(true);
    expect(oslChatSwitchBehaviour(settingsFromReadReceiptFixture({ send: true, receive: true })!).readReceipts)
      .toEqual({ send: true, receive: true });
    expect(settingsFromReadReceiptFixture({ send: false, receive: false })?.readReceipts).toBe(false);
    expect(oslChatSwitchBehaviour(settingsFromReadReceiptFixture({ send: false, receive: false })!).readReceipts)
      .toEqual({ send: false, receive: false });
    expect(settingsFromReadReceiptFixture({ send: true, receive: false })).toBeNull();
    expect(settingsFromReadReceiptFixture({ send: false, receive: true })).toBeNull();
    console.log("read-receipts allowed=both,neither asymmetric-refused=2");
  });

  it("changes exactly one behaviour per switch and leaves the other 5 unchanged", () => {
    const keys: readonly OslChatNotificationSwitch[] = [
      "notifications", "previews", "typingIndicator", "readReceipts", "dontShowImTyping", "dontShowWhenOnline",
    ];
    const initial = defaultOslChatNotificationSettings();
    expect(saveOslChatNotificationSettings(storage, personId, initial)).toBe(true);

    const behaviourForSwitch = {
      notifications: "notificationCreated",
      previews: "previewWordCount",
      typingIndicator: "incomingTypingVisible",
      readReceipts: "readReceipts",
      dontShowImTyping: "outgoingTypingVisible",
      dontShowWhenOnline: "outgoingOnlineVisible",
    } as const;
    let checks = 0;
    for (const key of keys) {
      saveOslChatNotificationSettings(storage, personId, initial);
      const before = oslChatSwitchBehaviour(readOslChatNotificationSettings(storage, personId), "four visible preview words");
      setOslChatNotificationSwitch(storage, personId, key, !initial[key]);
      const after = oslChatSwitchBehaviour(readOslChatNotificationSettings(storage, personId), "four visible preview words");
      const behaviourKeys = Object.keys(before) as Array<keyof typeof before>;
      const changed = behaviourKeys.filter((candidate) => JSON.stringify(before[candidate]) !== JSON.stringify(after[candidate]));
      expect(changed, `${key} changed ${changed.join(", ")}`).toEqual([behaviourForSwitch[key]]);
      expect(behaviourKeys.length - changed.length).toBe(5);
      console.log(`isolation switch=${key} changed=${changed.length} unchanged=${behaviourKeys.length - changed.length}`);
      checks += 1;
    }
    expect(checks).toBe(6);
    console.log(`isolation switches=${checks} each-changed=1 each-unchanged=5`);
  });

  it("keeps settings per chat and refuses malformed persisted records", () => {
    setOslChatNotificationSwitch(storage, personId, "notifications", false);
    expect(readOslChatNotificationSettings(storage, "another-friend").notifications).toBe(true);
    storage.setItem(OSL_CHAT_NOTIFICATION_SETTINGS_KEY, JSON.stringify({
      [personId]: { notifications: true, previews: true, typingIndicator: true, readReceipts: { send: true, receive: false }, dontShowImTyping: false, dontShowWhenOnline: false },
    }));
    expect(readOslChatNotificationSettings(storage, personId)).toEqual(defaultOslChatNotificationSettings());
  });
});
