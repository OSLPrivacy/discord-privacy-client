import { describe, expect, it } from "vitest";
import {
  FIRST_PARTY_OSL_SERVICE_SURFACES,
  firstPartyOslServiceSurface,
  oslChatsViewMarkup,
  type FirstPartyOslServiceSurface,
} from "./osl-chats-view";

describe("first-party OSL service surface", () => {
  it("Define first-party OSL service surface contracts", () => {
    const chat = firstPartyOslServiceSurface("osl-chat");
    const expected = {
      surfaceId: "osl-chat",
      destination: "Inbox",
      displayName: "OSL Chat",
      participantScope: "verified_friend",
      accountScope: "active_friend_only",
      plaintextBoundary: "osl_controlled_composer",
      sendAuthority: "explicit_user_action",
      historyVisibility: "local_osl_history",
      externalProvider: false,
    } satisfies FirstPartyOslServiceSurface;

    expect(FIRST_PARTY_OSL_SERVICE_SURFACES).toEqual([expected]);
    expect(chat).toBe(FIRST_PARTY_OSL_SERVICE_SURFACES[0]);
    expect(chat.externalProvider).toBe(false);
    expect(chat.participantScope).toBe("verified_friend");
    expect(chat.accountScope).toBe("active_friend_only");
    expect(chat.sendAuthority).toBe("explicit_user_action");

    const markup = oslChatsViewMarkup({
      friends: [{
        personId: "friend-1",
        nickname: "Rose",
        verified: true,
        ready: true,
        preview: "See you soon",
        previewVisible: true,
        unreadCount: 0,
      }],
      activePersonId: "friend-1",
      messages: [],
      draft: "Hello",
      busy: false,
    });
    expect(markup).toContain("OSL direct chat with Rose");
    expect(markup).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter|Discord|Telegram|Signal/iu);
  });
});
