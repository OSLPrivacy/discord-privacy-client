import { afterEach, describe, expect, it, vi } from "vitest";
import { oslChatsViewMarkup, type OslChatFriend, type OslChatsViewModel } from "./osl-chats-view";
import {
  OSL_CHAT_TYPING_STOP_GRACE_MS,
  createOslChatTypingController,
  oslChatTypingSettingsMarkup,
  type OslChatTypingPreferences,
  type OslChatTypingSignal,
} from "./typing-indicator";

const friend: OslChatFriend = {
  personId: "fixture-friend",
  nickname: "Rose Field",
  verified: true,
  ready: true,
  preview: null,
  previewVisible: true,
  unreadCount: 0,
};

function view(typingPersonId: string | null): string {
  const model: OslChatsViewModel = {
    friends: [friend],
    activePersonId: friend.personId,
    messages: [{
      messageId: "fixture-message",
      direction: "incoming",
      body: "Before the indicator",
      state: "received",
      timestampLabel: "Now",
    }],
    draft: "",
    busy: false,
    typingPersonId,
  };
  return oslChatsViewMarkup(model);
}

function controller(
  preferences: OslChatTypingPreferences,
  sent: OslChatTypingSignal[] = [],
) {
  return createOslChatTypingController({ preferences, send: (signal) => sent.push(signal) });
}

afterEach(() => {
  vi.useRealTimers();
});

describe("TASK 5012 typing indicator", () => {
  it("turns one fixture typing signal into exactly one faded avatar and three dots at the bottom of the thread", () => {
    const typing = controller({ hideOwnTyping: false, showIncomingTyping: true });
    typing.receive({ personId: friend.personId, typing: true });
    const markup = view(typing.incomingPersonId());
    const fadedAvatars = markup.match(/data-faded-avatar="true"/gu)?.length ?? 0;
    const dots = markup.match(/data-typing-dot/gu)?.length ?? 0;

    expect(fadedAvatars).toBe(1);
    expect(dots).toBe(3);
    expect(markup).toContain('aria-label="Rose Field is typing"');
    expect(markup.indexOf("fixture-message")).toBeLessThan(markup.indexOf("data-osl-chat-typing-indicator"));
    expect(markup.indexOf("data-osl-chat-typing-indicator")).toBeLessThan(markup.indexOf("osl-chat-composer"));
    console.info(`TASK_5012_FIXTURE faded_avatars=${fadedAvatars} dots=${dots}`);
  });

  it("removes the indicator 750 ms after the fixture stop signal", () => {
    vi.useFakeTimers();
    const typing = controller({ hideOwnTyping: false, showIncomingTyping: true });
    typing.receive({ personId: friend.personId, typing: true });
    typing.receive({ personId: friend.personId, typing: false });

    vi.advanceTimersByTime(OSL_CHAT_TYPING_STOP_GRACE_MS - 1);
    expect(view(typing.incomingPersonId())).toContain("data-osl-chat-typing-indicator");
    vi.advanceTimersByTime(1);
    expect(view(typing.incomingPersonId())).not.toContain("data-osl-chat-typing-indicator");
    expect(OSL_CHAT_TYPING_STOP_GRACE_MS).toBeLessThan(1_000);
    console.info(`TASK_5012_STOP disappeared_ms=${OSL_CHAT_TYPING_STOP_GRACE_MS} under_1_second=true`);
  });

  it("sends zero typing signals when Don't show when I'm typing is on", () => {
    const sent: OslChatTypingSignal[] = [];
    const typing = controller({ hideOwnTyping: true, showIncomingTyping: true }, sent);
    typing.localDraftChanged(friend.personId, true);
    typing.localDraftChanged(friend.personId, true);
    typing.localDraftChanged(friend.personId, false);

    expect(sent).toHaveLength(0);
    const settings = oslChatTypingSettingsMarkup({ hideOwnTyping: true, showIncomingTyping: true });
    expect(settings).toMatch(/id="osl-chat-hide-own-typing" type="checkbox" checked/u);
    console.info(`TASK_5012_OUTGOING hide_own_typing=on signals_sent=${sent.length}`);
  });

  it("never renders an incoming indicator while its switch is off", () => {
    const typing = controller({ hideOwnTyping: false, showIncomingTyping: false });
    typing.receive({ personId: friend.personId, typing: true });
    const markup = view(typing.incomingPersonId());
    const indicators = markup.match(/data-osl-chat-typing-indicator/gu)?.length ?? 0;

    expect(typing.incomingPersonId()).toBeNull();
    expect(indicators).toBe(0);
    const settings = oslChatTypingSettingsMarkup({ hideOwnTyping: false, showIncomingTyping: false });
    expect(settings).not.toMatch(/id="osl-chat-show-incoming-typing" type="checkbox" checked/u);
    console.info(`TASK_5012_INCOMING show_incoming_typing=off indicators=${indicators}`);
  });
});
