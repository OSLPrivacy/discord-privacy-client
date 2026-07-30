import { describe, expect, it } from "vitest";
import {
  firstPartyOslSurfaceContracts,
  OSL_PRIMARY_DESTINATIONS,
} from "./osl-chats-view";

describe("first-party OSL service surface contracts", () => {
  it("src/first-party-osl-surface.test.ts", () => {
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
});
