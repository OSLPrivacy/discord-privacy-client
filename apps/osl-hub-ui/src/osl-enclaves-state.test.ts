import { describe, expect, it, vi } from "vitest";
import {
  blockSpaceMember,
  channelAttention,
  createOslEnclavesLocalState,
  emptyEnclaveLocalFilters,
  hideSpaceChannel,
  markChannelRead,
  muteSpaceChannel,
  shouldNotifyForSpaceMessage,
  visibleSpaceChannels,
  visibleSpaceMessages,
} from "./osl-enclaves-state";
const messages = [
  { messageId: "outgoing", channelId: "general", localSequence: 1, incoming: false, mentionsLocalUser: false },
  { messageId: "hello", channelId: "general", localSequence: 2, incoming: true, mentionsLocalUser: false },
  { messageId: "ping", channelId: "general", localSequence: 3, incoming: true, mentionsLocalUser: true },
  { messageId: "private-ping", channelId: "private", localSequence: 4, incoming: true, mentionsLocalUser: true },
] as const;

describe("OSL Enclaves local unread state", () => {
  it("derives unread and mention counts from local messages, then advances only this device's frontier", () => {
    const firstDevice = createOslEnclavesLocalState();
    const secondDevice = createOslEnclavesLocalState();

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
      const state = createOslEnclavesLocalState();
      const read = markChannelRead(messages, state, "general");

      expect(channelAttention(messages, read, "general")).toEqual({ unreadCount: 0, mentionCount: 0 });
      expect(fetch).not.toHaveBeenCalled();
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

describe("Enclave local moderation filters", () => {
  it("hides channels and blocked members only on this device", () => {
    let filters = emptyEnclaveLocalFilters();
    filters = hideSpaceChannel(filters, "ops");
    filters = blockSpaceMember(filters, "member-abusive");

    expect(visibleSpaceChannels(filters, ["general", "ops", "random"]))
      .toEqual(["general", "random"]);
    expect(visibleSpaceMessages(filters, [
      { id: "a", channelId: "general", senderId: "member-safe" },
      { id: "b", channelId: "general", senderId: "member-abusive" },
    ])).toEqual([{ id: "a", channelId: "general", senderId: "member-safe" }]);
  });

  it("keeps muted content visible while suppressing only this device's notifications", () => {
    let filters = emptyEnclaveLocalFilters();
    filters = muteSpaceChannel(filters, "announcements");
    filters = blockSpaceMember(filters, "member-abusive");

    expect(visibleSpaceMessages(filters, [
      { id: "a", channelId: "announcements", senderId: "member-safe" },
    ])).toEqual([{ id: "a", channelId: "announcements", senderId: "member-safe" }]);
    expect(shouldNotifyForSpaceMessage(filters, { id: "a", channelId: "announcements", senderId: "member-safe" })).toBe(false);
    expect(shouldNotifyForSpaceMessage(filters, { id: "b", channelId: "general", senderId: "member-abusive" })).toBe(false);
    expect(shouldNotifyForSpaceMessage(filters, { id: "c", channelId: "general", senderId: "member-safe" })).toBe(true);
  });

  it("cannot serialize a block list into a relay or member request", () => {
    const filters = blockSpaceMember(emptyEnclaveLocalFilters(), "member-abusive");

    expect(JSON.stringify(filters)).toBe("{}");
  });
});
