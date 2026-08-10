import { describe, expect, it } from "vitest";
import {
  DEFAULT_CHAT_BACKGROUND_SETTINGS,
  loadChatBackgroundSettings,
  saveChatBackgroundSettings,
  type ChatBackgroundSettings,
} from "./chat-background-pane";

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => { values.set(key, value); },
    removeItem: (key) => { values.delete(key); },
    clear: () => { values.clear(); },
    key: (index) => [...values.keys()][index] ?? null,
    get length() { return values.size; },
  } as Storage;
}

function choice(background: ChatBackgroundSettings["background"], scope: ChatBackgroundSettings["scope"]): ChatBackgroundSettings {
  return { ...DEFAULT_CHAT_BACKGROUND_SETTINGS, background, scope };
}

describe("chat background scope persistence", () => {
  it("keeps This chat only in A, then makes Every chat visible to A and B", () => {
    const storage = memoryStorage();

    saveChatBackgroundSettings("A", choice("ember", "chat"), storage);
    const afterChatOnlyA = loadChatBackgroundSettings("A", storage);
    const afterChatOnlyB = loadChatBackgroundSettings("B", storage);
    console.log(`TASK5064 after_this_chat_only A=${afterChatOnlyA.background}/${afterChatOnlyA.scope} B=${afterChatOnlyB.background}/${afterChatOnlyB.scope}`);
    expect(afterChatOnlyA.background).toBe("ember");
    expect(afterChatOnlyB.background).toBe("none");

    saveChatBackgroundSettings("A", choice("dusk", "every-chat"), storage);
    const afterEveryChatA = loadChatBackgroundSettings("A", storage);
    const afterEveryChatB = loadChatBackgroundSettings("B", storage);
    console.log(`TASK5064 after_every_chat A=${afterEveryChatA.background}/${afterEveryChatA.scope} B=${afterEveryChatB.background}/${afterEveryChatB.scope}`);
    expect(afterEveryChatA.background).toBe("dusk");
    expect(afterEveryChatB.background).toBe("dusk");
  });
});
