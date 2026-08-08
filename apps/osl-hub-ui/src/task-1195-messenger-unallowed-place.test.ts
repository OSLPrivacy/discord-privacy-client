import { describe, expect, it } from "vitest";
import {
  messengerWhitelistControlsMarkup,
  type MessengerAllowedPlace,
  type MessengerVerificationState,
} from "./messenger-whitelist-controls";

const goodPlace: MessengerAllowedPlace = {
  app: "messenger",
  account: "messenger-alice-1195",
  kind: "direct_message",
  stableId: "messenger-allowed-1195",
  personName: "Bob",
  placeName: "Bob's direct message",
  allowed: true,
};

const reciprocal: MessengerVerificationState = {
  app: "messenger",
  kind: "direct_message",
  firstAccount: "messenger-alice-1195",
  secondAccount: "messenger-bob-1195",
  firstToSecondAllowed: true,
  secondToFirstAllowed: true,
  state: "two-way",
  verificationTicked: true,
};

const controlCount = (markup: string): number =>
  (markup.match(/data-messenger-whitelist-controls(?=[\s>])/gu) ?? []).length;

describe("TASK1195 Messenger unallowed places", () => {
  it("refuses changed conversation place values by name without changing the good place", () => {
    const original = messengerWhitelistControlsMarkup(goodPlace, reciprocal);
    expect(controlCount(original)).toBe(1);

    const refusedPlaceValues = ["direct-message", "group-chat", "community-conversation"];
    for (const placeValue of refusedPlaceValues) {
      const openedConversation = messengerWhitelistControlsMarkup(
        { ...goodPlace, kind: placeValue },
        reciprocal,
      );
      expect(openedConversation, `${placeValue} must be refused by name`).toBe("");
      expect(controlCount(openedConversation), `${placeValue} OSL control count`).toBe(0);
      console.log(`TASK1195 refused_place=${placeValue} osl_controls=0`);
    }

    const restored = messengerWhitelistControlsMarkup(goodPlace, reciprocal);
    expect(restored).toBe(original);
    expect(controlCount(restored)).toBe(1);
    console.log("TASK1195 good_place=messenger-allowed-1195 initial_osl_controls=1 refused_by_name=direct-message,group-chat,community-conversation final_osl_controls=1");
  });
});
