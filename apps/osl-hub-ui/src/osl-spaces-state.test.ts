import { describe, expect, it, vi } from "vitest";
import {
  channelAttention,
  createOslSpacesLocalState,
  markChannelRead,
} from "./osl-spaces-state";

const messages = [
  { messageId: "outgoing", channelId: "general", localSequence: 1, incoming: false, mentionsLocalUser: false },
  { messageId: "hello", channelId: "general", localSequence: 2, incoming: true, mentionsLocalUser: false },
  { messageId: "ping", channelId: "general", localSequence: 3, incoming: true, mentionsLocalUser: true },
  { messageId: "private-ping", channelId: "private", localSequence: 4, incoming: true, mentionsLocalUser: true },
] as const;

describe("OSL Spaces local unread state", () => {
  it("derives unread and mention counts from local messages, then advances only this device's frontier", () => {
    const firstDevice = createOslSpacesLocalState();
    const secondDevice = createOslSpacesLocalState();

    expect(channelAttention(messages, firstDevice, "general")).toEqual({ unreadCount: 2, mentionCount: 1 });

    const afterRead = markChannelRead(messages, firstDevice, "general");
    expect(channelAttention(messages, afterRead, "general")).toEqual({ unreadCount: 0, mentionCount: 0 });
    expect(channelAttention(messages, secondDevice, "general")).toEqual({ unreadCount: 2, mentionCount: 1 });

    const withNewMessage = [...messages, {
      messageId: "follow-up", channelId: "general", localSequence: 5, incoming: true, mentionsLocalUser: true,
    }];
    expect(channelAttention(withNewMessage, afterRead, "general")).toEqual({ unreadCount: 1, mentionCount: 1 });
    expect(channelAttention(withNewMessage, afterRead, "private")).toEqual({ unreadCount: 1, mentionCount: 1 });
  });

  it("does not contact a relay when calculating or marking read state", () => {
    const fetch = vi.fn();
    vi.stubGlobal("fetch", fetch);
    try {
      const state = createOslSpacesLocalState();
      const read = markChannelRead(messages, state, "general");

      expect(channelAttention(messages, read, "general")).toEqual({ unreadCount: 0, mentionCount: 0 });
      expect(fetch).not.toHaveBeenCalled();
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
