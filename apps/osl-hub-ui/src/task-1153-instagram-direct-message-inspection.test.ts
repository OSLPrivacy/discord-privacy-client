import { describe, expect, it } from "vitest";
import {
  inspectInstagramAllowedPlace,
  type InstagramAllowedPlace,
  type InstagramPlaceReader,
  type InstagramVerificationState,
} from "./instagram-whitelist-controls";

type FixtureKind = "direct_message" | "group_chat";

const noOpReader = process.env.TASK1153_INSTAGRAM_PLACE_READER === "noop";

function fixture(kind: FixtureKind): InstagramAllowedPlace {
  return {
    app: "instagram",
    account: "instagram-alice-1153",
    kind,
    stableId: `instagram:instagram-alice-1153:${kind}:instagram-place-1153`,
    personName: kind === "direct_message" ? "Bob" : "Bob, Chen, and Devon",
    placeName: kind === "direct_message" ? "Bob's direct message" : "Project group DM",
    allowed: true,
  };
}

function reader(kind: FixtureKind): InstagramPlaceReader {
  return noOpReader ? () => undefined : () => fixture(kind);
}

function reciprocal(kind: FixtureKind): InstagramVerificationState {
  return {
    app: "instagram",
    kind,
    firstAccount: "instagram-alice-1153",
    secondAccount: "instagram-place-1153",
    firstToSecondAllowed: true,
    secondToFirstAllowed: true,
    savedDirections: 2,
    verificationState: "visible",
    state: "two-way",
  };
}

describe("TASK1153 Instagram direct-message and group-DM inspection", () => {
  it("directly reads an allowed direct message and its control", () => {
    const inspected = inspectInstagramAllowedPlace(reader("direct_message"), reciprocal("direct_message"));

    expect(inspected).not.toBeNull();
    expect(inspected!.kind).toBe("direct_message");
    expect(inspected!.controls).toContain("data-instagram-whitelist-controls");
    const controls = (inspected!.controls.match(/data-instagram-whitelist-controls/gu) ?? []).length;
    expect(controls).toBe(1);
    console.log(`TASK1153 direct_message_kind=${inspected!.kind} direct_message_controls=${controls}`);
  });

  it("directly reads an allowed group DM and keeps its controls absent", () => {
    const inspected = inspectInstagramAllowedPlace(reader("group_chat"), reciprocal("group_chat"));

    expect(inspected).not.toBeNull();
    expect(inspected!.kind).toBe("group_chat");
    const controls = (inspected!.controls.match(/data-instagram-whitelist-controls/gu) ?? []).length;
    expect(controls).toBe(0);
    console.log(`TASK1153 group_dm_kind=${inspected!.kind} group_dm_controls=${controls}`);
  });
});
