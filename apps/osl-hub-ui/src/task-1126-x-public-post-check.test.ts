import { describe, expect, it } from "vitest";
import { type XAllowedPlace } from "./x-whitelist-controls";

type FixtureKind = "direct_message" | "group_direct_message" | "public_post" | "reply";
type WarningState = "private" | "public" | "reply";

interface XComposerFixture {
  name: string;
  place: XAllowedPlace;
  composer: Readonly<{
    role: "textbox";
    label: string;
    warningState: WarningState;
  }>;
}

interface XComposerInspection {
  kind: FixtureKind;
  warningState: WarningState;
}

const fixture = (
  kind: FixtureKind,
  label: string,
  warningState: WarningState,
): XComposerFixture => ({
  name: kind.replaceAll("_", "-"),
  place: {
    app: "x",
    account: "x-alice-1126",
    kind,
    stableId: `x:x-alice-1126:${kind}:fixture-1126`,
    personName: "Public audience",
    placeName: `X ${kind.replaceAll("_", " ")} composer`,
    allowed: true,
  },
  composer: { role: "textbox", label, warningState },
});

const fixtures: readonly XComposerFixture[] = [
  fixture("public_post", "Post text", "public"),
  fixture("reply", "Post your reply", "reply"),
  fixture("direct_message", "Start a message", "private"),
  fixture("group_direct_message", "Start a message", "private"),
];

function inspectAllowedXComposer(candidate: XComposerFixture): XComposerInspection {
  if (candidate.place.app !== "x" || !candidate.place.allowed || candidate.composer.role !== "textbox") {
    throw new Error(`TASK1126 ${candidate.name} is not an allowed X composer`);
  }

  const expectedSurface: Record<FixtureKind, Readonly<{ label: string; warningState: WarningState }>> = {
    public_post: { label: "Post text", warningState: "public" },
    reply: { label: "Post your reply", warningState: "reply" },
    direct_message: { label: "Start a message", warningState: "private" },
    group_direct_message: { label: "Start a message", warningState: "private" },
  };
  const expected = expectedSurface[candidate.place.kind as FixtureKind];
  if (expected === undefined
    || candidate.composer.label !== expected.label
    || candidate.composer.warningState !== expected.warningState) {
    throw new Error(`TASK1126 ${candidate.name} composer markers do not match ${candidate.place.kind}`);
  }

  return {
    kind: candidate.place.kind as FixtureKind,
    warningState: candidate.composer.warningState,
  };
}

function inspectAllowedXPublicPost(candidate: XComposerFixture): Readonly<{
  kind: "public_post";
  warningState: "public";
}> {
  const inspection = inspectAllowedXComposer(candidate);
  if (inspection.kind !== "public_post") {
    throw new Error(`TASK1126 expected public_post fixture, received ${inspection.kind}`);
  }
  if (inspection.warningState !== "public") {
    throw new Error(`TASK1126 public_post fixture returned ${inspection.warningState} warning state`);
  }
  return { kind: inspection.kind, warningState: inspection.warningState };
}

describe("TASK1126 X public-post composer inspection", () => {
  it("returns exactly public_post and the public warning state", () => {
    const inspection = inspectAllowedXPublicPost(fixtures[0]);

    expect(inspection).toEqual({ kind: "public_post", warningState: "public" });
    console.log(`TASK1126_PUBLIC_KIND=${inspection.kind} TASK1126_PUBLIC_WARNING_STATE=${inspection.warningState}`);
  });

  it("keeps the reply fixture distinct and never returns public_post for another fixture", () => {
    const reply = inspectAllowedXComposer(fixtures[1]);
    const otherPublicPostResults = fixtures
      .slice(1)
      .map(inspectAllowedXComposer)
      .filter(({ kind }) => kind === "public_post");

    expect(reply).toEqual({ kind: "reply", warningState: "reply" });
    expect(otherPublicPostResults).toEqual([]);
    console.log(`TASK1126_REPLY_KIND=${reply.kind} TASK1126_REPLY_WARNING_STATE=${reply.warningState} TASK1126_OTHER_PUBLIC_POST_RESULTS=${otherPublicPostResults.length}`);
  });

  it("rejects the reply fixture when used as the public-post check", () => {
    expect(() => inspectAllowedXPublicPost(fixtures[1])).toThrow(
      "TASK1126 expected public_post fixture, received reply",
    );
    console.log("TASK1126_REPLY_AS_PUBLIC_REJECTED=true");
  });

  it("runs the selected public-post check", () => {
    const selected = process.env.TASK1126_CHECK_FIXTURE === "reply" ? fixtures[1] : fixtures[0];
    const inspection = inspectAllowedXPublicPost(selected);
    expect(inspection).toEqual({ kind: "public_post", warningState: "public" });
    console.log(`TASK1126_SELECTED_KIND=${inspection.kind} TASK1126_SELECTED_WARNING_STATE=${inspection.warningState}`);
  });
});
