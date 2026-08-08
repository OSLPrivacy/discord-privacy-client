import { describe, expect, it } from "vitest";
import {
  messengerCommunityControlsMarkup,
  messengerCommunityControlsVisible,
  type MessengerAllowedPlace,
} from "./messenger-whitelist-controls";

const fixture = (kind: string, allowed = true): MessengerAllowedPlace => ({
  app: "messenger",
  account: "messenger-alice-1190",
  kind,
  stableId: `messenger:messenger-alice-1190:${kind}:place-1190`,
  personName: "Community fixture",
  placeName: `Messenger ${kind} fixture`,
  allowed,
});

type CommunityInspection = Readonly<{ kind: "community"; controls: string }>;

/** The selected fixture must be the allowed Messenger community, never a group chat. */
function inspectAllowedMessengerCommunity(place: MessengerAllowedPlace): CommunityInspection {
  if (place.app !== "messenger" || place.kind !== "community") {
    throw new Error(`TASK1190 expected community fixture, received ${place.kind}`);
  }
  const controls = messengerCommunityControlsMarkup(place);
  if (!messengerCommunityControlsVisible(place) || !controls.includes("data-messenger-community-controls")) {
    throw new Error("TASK1190 allowed community fixture returned no community controls");
  }
  return { kind: "community", controls };
}

describe("TASK1190 Messenger community inspection", () => {
  it("returns exactly community kind and community controls for the allowed community fixture", () => {
    const inspection = inspectAllowedMessengerCommunity(fixture("community"));

    expect(inspection.kind).toBe("community");
    expect(inspection.controls).toContain("data-messenger-community-controls");
    expect(inspection.controls).toContain("data-messenger-community-toggle");
    console.log(`TASK1190_COMMUNITY_KIND=${inspection.kind} TASK1190_COMMUNITY_CONTROLS=${(inspection.controls.match(/data-messenger-community-controls/g) ?? []).length}`);
  });

  it("keeps a group-chat fixture distinct and gives every other fixture no community controls", () => {
    const groupChat = fixture("group_chat");
    const otherFixtures = [groupChat, fixture("direct_message"), fixture("community", false), fixture("room")];

    expect(groupChat.kind).not.toBe("community");
    expect(messengerCommunityControlsMarkup(groupChat)).toBe("");
    expect(otherFixtures.filter((place) => messengerCommunityControlsVisible(place))).toEqual([]);
    console.log(`TASK1190_GROUP_KIND=${groupChat.kind} TASK1190_NON_COMMUNITY_RETURNS=${otherFixtures.filter((place) => messengerCommunityControlsVisible(place)).length}`);
  });

  it("fails when the community check is pointed at the group-chat fixture", () => {
    expect(() => inspectAllowedMessengerCommunity(fixture("group_chat"))).toThrow(
      "TASK1190 expected community fixture, received group_chat",
    );
    console.log("TASK1190_GROUP_AS_COMMUNITY_REJECTED=true");
  });

  it("runs the selected community check", () => {
    const selected = process.env.TASK1190_CHECK_FIXTURE === "group_chat"
      ? fixture("group_chat")
      : fixture("community");
    const inspection = inspectAllowedMessengerCommunity(selected);
    expect(inspection.kind).toBe("community");
    console.log(`TASK1190_SELECTED_KIND=${inspection.kind}`);
  });
});
