import { expect, test } from "vitest";
import {
  signalWhitelistControlsMarkup,
  type SignalAllowedPlace,
} from "./signal-whitelist-controls";

const ACCOUNT = "signal-group-owner-1053";
const PEER = "signal-group-peer-1053";

interface Fixture {
  name: string;
  place: SignalAllowedPlace;
}

const fixtures: readonly Fixture[] = [
  {
    name: "allowed-signal-group",
    place: {
      app: "signal",
      account: ACCOUNT,
      kind: "group_chat",
      stableId: `signal:${ACCOUNT}:group_chat:${PEER}`,
      personName: "1053 Signal group",
      placeName: "1053 Signal group",
      allowed: true,
    },
  },
  {
    name: "direct-message",
    place: {
      app: "signal",
      account: ACCOUNT,
      kind: "direct_message",
      stableId: `signal:${ACCOUNT}:direct_message:${PEER}`,
      personName: "1053 direct-message peer",
      placeName: "1053 direct message",
      allowed: true,
    },
  },
  {
    name: "signal-story",
    place: {
      app: "signal",
      account: ACCOUNT,
      kind: "story",
      stableId: `signal:${ACCOUNT}:story:${PEER}`,
      personName: "1053 Signal story",
      placeName: "1053 Signal story",
      allowed: true,
    },
  },
];

function inspect(fixture: Fixture): { kind: string; controls: number } {
  const markup = signalWhitelistControlsMarkup(fixture.place, null);
  return {
    kind: fixture.place.kind,
    controls: (markup.match(/data-signal-whitelist-controls/gu) ?? []).length,
  };
}

function fixtureNamed(name: string): Fixture {
  const fixture = fixtures.find((candidate) => candidate.name === name);
  expect(fixture, `TASK1053 unknown fixture=${name}`).toBeDefined();
  return fixture!;
}

test("TASK1053 directly checks the allowed Signal group fixture", () => {
  const requested = process.env.TASK1053_FIXTURE ?? "allowed-signal-group";
  const result = inspect(fixtureNamed(requested));

  // This is deliberately an exact group check.  TASK1053_FIXTURE=direct-message
  // is the anti-vacuity control: it must fail here rather than silently testing
  // a direct message in place of the group fixture.
  expect(result).toEqual({ kind: "group_chat", controls: 0 });
  console.log(`TASK1053 fixture=${requested} kind=${result.kind} controls=${result.controls}`);
});

test("TASK1053 direct-message fixture is a different kind and no other fixture is group", () => {
  const direct = inspect(fixtureNamed("direct-message"));
  const groupFixtures = fixtures.filter(({ place }) => place.kind === "group_chat");
  const nonGroupFixtures = fixtures.filter(({ place }) => place.kind !== "group_chat");

  expect(direct.kind).toBe("direct_message");
  expect(direct.kind).not.toBe("group_chat");
  expect(groupFixtures.map(({ name }) => name)).toEqual(["allowed-signal-group"]);
  expect(nonGroupFixtures.every(({ place }) => place.kind !== "group_chat")).toBe(true);
  console.log(
    `TASK1053 direct_fixture_kind=${direct.kind} direct_fixture_controls=${direct.controls} `
      + `group_fixture_count=${groupFixtures.length} other_group_fixture_count=0`,
  );
});
