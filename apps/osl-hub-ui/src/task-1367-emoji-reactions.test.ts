// TASK 1367 -- build emoji reaction display
//
// Reactions render as chips with emoji and count under messages.
// This test validates that a message with two emoji reactions
// renders with exactly those chips, displaying each emoji and its count.

import { describe, expect, it } from "vitest";
import { oslChatsViewMarkup } from "./osl-chats-view";
import type { OslChatsViewModel, OslChatMessage, OslChatFriend } from "./osl-chats-view";

function createMessage(overrides: Partial<OslChatMessage> = {}): OslChatMessage {
  return {
    messageId: "msg-123",
    direction: "incoming",
    body: "Hello there!",
    state: "received",
    timestampLabel: "2:30 PM",
    ...overrides,
  };
}

function createFriend(overrides: Partial<OslChatFriend> = {}): OslChatFriend {
  return {
    personId: "person-1",
    nickname: "Alice",
    verified: true,
    ready: true,
    preview: null,
    previewVisible: true,
    unreadCount: 0,
    handshakeConfirmed: true,
    ...overrides,
  };
}

describe("TASK 1367 emoji reaction display", () => {
  it("renders two emoji chips with exact counts under a message", () => {
    const friend = createFriend();
    const message = createMessage({
      reactions: [
        { emoji: "👍", count: 3, mine: false },
        { emoji: "❤️", count: 2, mine: true },
      ],
    });
    const model: OslChatsViewModel = {
      friends: [friend],
      activePersonId: friend.personId,
      messages: [message],
      draft: "",
      busy: false,
    };

    const markup = oslChatsViewMarkup(model);

    // Validate the first reaction chip: 👍 with count 3
    expect(markup).toContain('data-osl-chat-emoji="👍"');
    expect(markup).toContain('aria-pressed="false"');
    const firstChipMatch = markup.match(
      /<button[^>]*data-osl-chat-emoji="👍"[^>]*>[\s\S]*?<span>👍<\/span>\s*<span>3<\/span>[\s\S]*?<\/button>/
    );
    expect(firstChipMatch).toBeTruthy();

    // Validate the second reaction chip: ❤️ with count 2 and is-mine class
    expect(markup).toContain('data-osl-chat-emoji="❤️"');
    expect(markup).toContain('class="osl-chat-reaction is-mine"');
    expect(markup).toContain('aria-pressed="true"');
    const secondChipMatch = markup.match(
      /<button[^>]*data-osl-chat-emoji="❤️"[^>]*>[\s\S]*?<span>❤️<\/span>\s*<span>2<\/span>[\s\S]*?<\/button>/
    );
    expect(secondChipMatch).toBeTruthy();

    // Validate message ID is attached to reactions
    expect(markup).toContain('data-osl-chat-reaction="msg-123"');
  });

  it("renders the add reaction button with 👍 default emoji", () => {
    const friend = createFriend();
    const message = createMessage();
    const model: OslChatsViewModel = {
      friends: [friend],
      activePersonId: friend.personId,
      messages: [message],
      draft: "",
      busy: false,
    };

    const markup = oslChatsViewMarkup(model);

    // The add reaction button should have the default 👍 emoji
    expect(markup).toContain('class="osl-chat-reaction is-add"');
    expect(markup).toContain('aria-label="Add reaction"');
    expect(markup).toContain('>👍</button>');
  });

  it("displays message with reactions in the correct container", () => {
    const friend = createFriend();
    const message = createMessage({
      reactions: [
        { emoji: "🎉", count: 1, mine: true },
      ],
    });
    const model: OslChatsViewModel = {
      friends: [friend],
      activePersonId: friend.personId,
      messages: [message],
      draft: "",
      busy: false,
    };

    const markup = oslChatsViewMarkup(model);

    // Validate the container structure: message article with reactions div
    expect(markup).toContain('class="osl-chat-message');
    expect(markup).toContain('class="osl-chat-reactions"');
    expect(markup).toContain('data-osl-chat-emoji="🎉"');
  });
});
