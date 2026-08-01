import { describe, expect, it } from "vitest";

import {
  blockSpaceMember,
  emptySpaceLocalFilters,
  hideSpaceChannel,
  muteSpaceChannel,
  shouldNotifyForSpaceMessage,
  visibleSpaceChannels,
  visibleSpaceMessages,
} from "./osl-spaces-state";

describe("Space local moderation filters", () => {
  it("hides channels and blocked members only on this device", () => {
    let filters = emptySpaceLocalFilters();
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
    let filters = emptySpaceLocalFilters();
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
    const filters = blockSpaceMember(emptySpaceLocalFilters(), "member-abusive");

    expect(JSON.stringify(filters)).toBe("{}");
  });
});
