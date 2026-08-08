// TASK 5011 -- reactions as flat chips with tap to toggle
//
// Reactions render as flat chips under the message (emoji + count, no names).
// Tapping a chip toggles the caller's own reaction: count moves by exactly
// one and the chip's "mine" mark flips with it.

import { describe, expect, it } from "vitest";
import { applyOslChatReactionToggle, oslChatsViewMarkup } from "./osl-chats-view";
import type { OslChatsViewModel, OslChatMessage, OslChatFriend, OslChatMessageReaction } from "./osl-chats-view";

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

function viewMarkup(reactions: readonly OslChatMessageReaction[]): string {
  const friend = createFriend();
  const message: OslChatMessage = {
    messageId: "msg-5011",
    direction: "incoming",
    body: "Reacting to this",
    state: "received",
    timestampLabel: "2:30 PM",
    reactions,
  };
  const model: OslChatsViewModel = {
    friends: [friend],
    activePersonId: friend.personId,
    messages: [message],
    draft: "",
    busy: false,
  };
  return oslChatsViewMarkup(model);
}

const THREE_KINDS: OslChatMessageReaction[] = [
  { emoji: "👍", count: 3, mine: false },
  { emoji: "❤️", count: 2, mine: true },
  { emoji: "🎉", count: 1, mine: false },
];

describe("TASK 5011 reaction chips render flat with tap-to-toggle", () => {
  it("shows exactly 3 flat chips with the right counts for a message with 3 reaction kinds", () => {
    const markup = viewMarkup(THREE_KINDS);
    const chipMatches = [...markup.matchAll(/class="osl-chat-reaction( is-mine)?"[^>]*data-osl-chat-reaction="msg-5011"/g)];
    expect(chipMatches).toHaveLength(3);
    for (const reaction of THREE_KINDS) {
      const chip = markup.match(
        new RegExp(`<button[^>]*data-osl-chat-emoji="${reaction.emoji}"[^>]*>[\\s\\S]*?<span>${reaction.emoji}</span>\\s*<span>${reaction.count}</span>[\\s\\S]*?</button>`)
      );
      expect(chip).toBeTruthy();
    }
  });

  it("marks the caller's own chip and only the caller's own chip", () => {
    const markup = viewMarkup(THREE_KINDS);
    expect(markup).toContain('data-osl-chat-emoji="❤️" data-osl-chat-reaction-mine="true" aria-pressed="true"');
    expect(markup).toContain('data-osl-chat-emoji="👍" data-osl-chat-reaction-mine="false" aria-pressed="false"');
    expect(markup).toContain('data-osl-chat-emoji="🎉" data-osl-chat-reaction-mine="false" aria-pressed="false"');
    expect(markup.match(/is-mine/g)).toHaveLength(1);
  });

  it("shows zero reacting account names anywhere on the chips", () => {
    const markup = viewMarkup(THREE_KINDS);
    const reactionsStart = markup.indexOf('class="osl-chat-reactions"');
    const reactionsBlock = markup.slice(reactionsStart, markup.indexOf("</div>", reactionsStart) + "</div>".length);
    for (const name of ["Alice", "person-1"]) {
      expect(reactionsBlock).not.toContain(name);
    }
  });

  it("tapping an existing chip you have not reacted to raises its count by exactly 1 and marks it yours", () => {
    const before = THREE_KINDS.find((r) => r.emoji === "👍")!;
    expect(before.mine).toBe(false);
    expect(before.count).toBe(3);

    const after = applyOslChatReactionToggle(THREE_KINDS, { emoji: "👍", added: true, removed: false });
    const toggled = after.find((r) => r.emoji === "👍")!;

    expect(toggled.count).toBe(before.count + 1);
    expect(toggled.mine).toBe(true);
    // The other two chips are untouched by the tap.
    expect(after.find((r) => r.emoji === "❤️")).toEqual({ emoji: "❤️", count: 2, mine: true });
    expect(after.find((r) => r.emoji === "🎉")).toEqual({ emoji: "🎉", count: 1, mine: false });
  });

  it("tapping your own chip again lowers its count by exactly 1 and unmarks it", () => {
    const mineChip = THREE_KINDS.find((r) => r.emoji === "❤️")!;
    expect(mineChip.mine).toBe(true);
    expect(mineChip.count).toBe(2);

    const after = applyOslChatReactionToggle(THREE_KINDS, { emoji: "❤️", added: false, removed: true });
    const toggled = after.find((r) => r.emoji === "❤️")!;

    expect(toggled.count).toBe(mineChip.count - 1);
    expect(toggled.mine).toBe(false);
  });

  it("removes the chip entirely once the last-owner's tap drops its count to 0", () => {
    const soleReaction: OslChatMessageReaction[] = [{ emoji: "🔥", count: 1, mine: true }];
    const after = applyOslChatReactionToggle(soleReaction, { emoji: "🔥", added: false, removed: true });
    expect(after.find((r) => r.emoji === "🔥")).toBeUndefined();
  });

  it("a fresh tap on a chip with no prior reactors creates a new chip marked yours at count 1", () => {
    const after = applyOslChatReactionToggle([], { emoji: "😂", added: true, removed: false });
    expect(after).toEqual([{ emoji: "😂", count: 1, mine: true }]);
  });
});
