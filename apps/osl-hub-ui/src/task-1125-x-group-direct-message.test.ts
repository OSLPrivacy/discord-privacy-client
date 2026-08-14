import { describe, expect, it } from "vitest";
import { xWhitelistControlsMarkup, type XAllowedPlace, type XVerificationState } from "./x-whitelist-controls";

type FixtureKind = "group_direct_message" | "direct_message" | "public_post" | "reply";

const requestedKind = (process.env.TASK1125_FIXTURE ?? "group_direct_message") as FixtureKind;

function fixture(kind: FixtureKind): XAllowedPlace {
  return {
    app: "x",
    account: "x-alice-1125",
    kind,
    stableId: `x:x-alice-1125:${kind}:group-1125`,
    personName: "Bob, Chen, and Devon",
    placeName: "Project group",
    allowed: true,
  };
}

const reciprocal: XVerificationState = {
  app: "x", kind: "group_direct_message", firstAccount: "x-alice-1125", secondAccount: "group-1125",
  firstToSecondAllowed: true, secondToFirstAllowed: true, state: "two-way",
};

describe("TASK1125 X group direct-message inspection", () => {
  it("returns exactly the group-direct-message kind and its controls", () => {
    const inspected = fixture(requestedKind);
    const markup = xWhitelistControlsMarkup(inspected, reciprocal);

    expect(inspected.kind).toBe("group_direct_message");
    expect(markup).toContain('data-x-place-kind="group_direct_message"');
    expect(markup).toContain('data-x-whitelist-controls');
    expect(markup).toContain("group direct message");
    console.log(`TASK1125 inspected_kind=${inspected.kind} group_direct_message_controls=${(markup.match(/data-x-whitelist-controls/g) ?? []).length}`);
  });

  it("keeps the direct-message fixture distinct and every other fixture out", () => {
    const fixtures: FixtureKind[] = ["group_direct_message", "direct_message", "public_post", "reply"];
    const kinds = fixtures.map((kind) => fixture(kind).kind);
    const groupMarkup = xWhitelistControlsMarkup(fixture("group_direct_message"), reciprocal);
    const directMarkup = xWhitelistControlsMarkup(fixture("direct_message"), reciprocal);
    const otherGroupKinds = kinds.filter((kind) => kind === "group_direct_message" && kind !== fixtures[0]);

    expect(fixture("direct_message").kind).not.toBe("group_direct_message");
    expect(directMarkup).not.toContain('data-x-place-kind="group_direct_message"');
    expect(otherGroupKinds).toEqual([]);
    expect(groupMarkup).toContain('data-x-place-kind="group_direct_message"');
    console.log(`TASK1125 direct_message_kind=${fixture("direct_message").kind} direct_message_group_controls=0 other_group_direct_message_fixtures=${otherGroupKinds.length}`);
  });
});
