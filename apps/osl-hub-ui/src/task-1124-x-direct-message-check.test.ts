import { describe, expect, it } from "vitest";
import {
  xDirectMessageControlsVisible,
  xWhitelistControlsMarkup,
  type XAllowedPlace,
  type XVerificationState,
} from "./x-whitelist-controls";

const verification: XVerificationState = {
  app: "x",
  kind: "direct_message",
  firstAccount: "x-alice-1124",
  secondAccount: "x-bob-1124",
  firstToSecondAllowed: true,
  secondToFirstAllowed: true,
  state: "two-way",
};

const fixture = (kind: string, allowed = true): XAllowedPlace => ({
  app: "x",
  account: "x-alice-1124",
  kind,
  stableId: `x:x-alice-1124:${kind}:x-bob-1124`,
  personName: "Bob",
  placeName: `Bob's ${kind}`,
  allowed,
});

type DirectMessageInspection = Readonly<{ kind: "direct_message"; controls: string }>;

/**
 * The check is deliberately narrower than the renderer: only an allowed X
 * direct message may produce its direct-message controls.
 */
function inspectAllowedXDirectMessage(place: XAllowedPlace): DirectMessageInspection {
  if (place.kind !== "direct_message") {
    throw new Error(`TASK1124 expected direct_message fixture, received ${place.kind}`);
  }
  const controls = xWhitelistControlsMarkup(place, verification);
  if (!xDirectMessageControlsVisible(place) || !controls.includes("data-x-whitelist-controls")) {
    throw new Error("TASK1124 allowed direct_message fixture returned no direct-message controls");
  }
  return { kind: "direct_message", controls };
}

describe("TASK1124 X direct-message inspection", () => {
  it("returns exactly direct_message and direct-message controls for the allowed direct fixture", () => {
    const inspection = inspectAllowedXDirectMessage(fixture("direct_message"));

    expect(inspection.kind).toBe("direct_message");
    expect(inspection.controls).toContain("data-x-whitelist-controls");
    expect(inspection.controls).toContain("data-x-whitelist-toggle");
    expect(inspection.controls).toContain("data-x-verification-tick=\"visible\"");
    console.log(`TASK1124_DIRECT_KIND=${inspection.kind} TASK1124_DIRECT_CONTROLS=${(inspection.controls.match(/data-x-whitelist-controls/g) ?? []).length}`);
  });

  it("keeps a group-direct-message fixture distinct and gives every non-direct fixture no direct controls", () => {
    const groupDirectMessage = fixture("group_direct_message");
    const otherFixtures = [groupDirectMessage, fixture("public_post"), fixture("reply"), fixture("direct_message", false)];

    expect(groupDirectMessage.kind).not.toBe("direct_message");
    expect(xWhitelistControlsMarkup(groupDirectMessage, verification)).toBe("");
    expect(otherFixtures.filter((place) => xDirectMessageControlsVisible(place))).toEqual([]);
    console.log(`TASK1124_GROUP_KIND=${groupDirectMessage.kind} TASK1124_NON_DIRECT_MESSAGE_RETURNS=${otherFixtures.filter((place) => xDirectMessageControlsVisible(place)).length}`);
  });

  it("fails when the direct-message check is pointed at the group-direct-message fixture", () => {
    const groupDirectMessage = fixture("group_direct_message");
    expect(() => inspectAllowedXDirectMessage(groupDirectMessage)).toThrow(
      "TASK1124 expected direct_message fixture, received group_direct_message",
    );
    console.log("TASK1124_GROUP_AS_DIRECT_REJECTED=true");
  });

  it("runs the selected direct-message check", () => {
    const selected = process.env.TASK1124_CHECK_FIXTURE === "group_direct_message"
      ? fixture("group_direct_message")
      : fixture("direct_message");
    const inspection = inspectAllowedXDirectMessage(selected);
    expect(inspection.kind).toBe("direct_message");
    console.log(`TASK1124_SELECTED_KIND=${inspection.kind}`);
  });
});
