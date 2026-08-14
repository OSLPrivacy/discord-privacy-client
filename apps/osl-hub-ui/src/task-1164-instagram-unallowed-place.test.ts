import { describe, expect, it } from "vitest";
import {
  instagramWhitelistControlsMarkup,
  type InstagramAllowedPlace,
  type InstagramVerificationState,
} from "./instagram-whitelist-controls";

const goodPlace: InstagramAllowedPlace = {
  app: "instagram",
  account: "instagram-alice-1164",
  kind: "direct_message",
  stableId: "instagram-allowed-1164",
  personName: "Bob",
  placeName: "Bob's direct message",
  allowed: true,
};

const reciprocal: InstagramVerificationState = {
  app: "instagram",
  kind: "direct_message",
  firstAccount: "instagram-alice-1164",
  secondAccount: "instagram-bob-1164",
  firstToSecondAllowed: true,
  secondToFirstAllowed: true,
  savedDirections: 2,
  verificationState: "visible",
  state: "two-way",
};

const controlCount = (markup: string): number =>
  (markup.match(/data-instagram-whitelist-controls(?=[\s>])/gu) ?? []).length;

describe("TASK1164 Instagram unallowed places", () => {
  it("refuses changed composer place values by name without changing the good place", () => {
    const original = instagramWhitelistControlsMarkup(goodPlace, reciprocal);
    expect(controlCount(original)).toBe(1);

    const refusedPlaceValues = ["direct-message", "post", "comment", "story-composer"];
    for (const placeValue of refusedPlaceValues) {
      const openedComposer = instagramWhitelistControlsMarkup(
        { ...goodPlace, kind: placeValue },
        reciprocal,
      );
      expect(openedComposer, `${placeValue} must be refused by name`).toBe("");
      expect(controlCount(openedComposer), `${placeValue} OSL control count`).toBe(0);
      console.log(`TASK1164 refused_place=${placeValue} osl_controls=0`);
    }

    const restored = instagramWhitelistControlsMarkup(goodPlace, reciprocal);
    expect(restored).toBe(original);
    expect(controlCount(restored)).toBe(1);
    console.log("TASK1164 good_place=instagram-allowed-1164 initial_osl_controls=1 refused_by_name=direct-message,post,comment,story-composer final_osl_controls=1");
  });
});
