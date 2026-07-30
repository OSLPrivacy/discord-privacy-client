import { describe, expect, it } from "vitest";
import {
  FIRST_PARTY_OSL_SERVICE_SURFACES,
  firstPartyOslServiceSurface,
  firstPartyOslSurfaceContracts,
  OSL_PRIMARY_DESTINATIONS,
  oslChatsViewMarkup,
  type FirstPartyOslServiceSurface,
} from "./osl-chats-view";

describe("first-party OSL service surface contracts", () => {
  it("keeps first-party service surfaces inside the fixed Inbox IA", () => {
    const surfaces = firstPartyOslSurfaceContracts();

    expect(OSL_PRIMARY_DESTINATIONS).toEqual([
      "Home",
      "Inbox",
      "People",
      "Privacy",
      "Activity",
      "Connections",
    ]);
    expect(surfaces.map((surface) => surface.id)).toEqual([
      "osl-chat",
      "osl-circles",
      "osl-mail",
    ]);
    expect(surfaces.every((surface) => surface.destination === "Inbox")).toBe(true);
    expect(surfaces.every((surface) => surface.externalPlatform === false)).toBe(true);
    expect(surfaces.every((surface) => surface.requiresConnectedService === false)).toBe(true);
    expect(surfaces.every((surface) => surface.missingCapability === "unavailable")).toBe(true);
    expect(surfaces.find((surface) => surface.id === "osl-chat")?.state).toBe("available");
    expect(surfaces.find((surface) => surface.id === "osl-circles")?.state).toBe("coming_later");
    expect(surfaces.find((surface) => surface.id === "osl-mail")?.state).toBe("coming_later");

    const visibleText = surfaces
      .flatMap((surface) => [
        surface.label,
        surface.primaryAction,
        surface.protectionScope,
        surface.state,
        surface.missingCapability,
      ])
      .join(" ");
    expect(visibleText).not.toMatch(
      /keyserver|ratchet|receipt|browser profile|provider adapter|ban-risk|\d+%\s+risk/iu,
    );
    expect(visibleText).not.toMatch(/Discord|Telegram|Signal|WhatsApp|Instagram|Snapchat|X|Messenger/iu);
  });

  it("defines the concrete OSL Chat surface and keeps markup product-facing", () => {
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
