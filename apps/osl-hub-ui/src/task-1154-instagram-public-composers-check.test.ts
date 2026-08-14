import { describe, expect, it } from "vitest";
import { type InstagramAllowedPlace } from "./instagram-whitelist-controls";

type InstagramFixtureKind = "direct_message" | "group_chat" | "public_post" | "comment" | "story";
type WarningState = "private" | "public";

interface InstagramComposerFixture {
  place: InstagramAllowedPlace;
  composer: Readonly<{
    role: "textbox";
    label: string;
    warningState: WarningState;
  }>;
}

interface InstagramSurfaceFixture {
  name: string;
  composers: readonly InstagramComposerFixture[];
}

interface InstagramComposerInspection {
  kind: InstagramFixtureKind;
  warningState: WarningState;
}

function composer(
  kind: InstagramFixtureKind,
  label: string,
  warningState: WarningState,
): InstagramComposerFixture {
  return {
    place: {
      app: "instagram",
      account: "instagram-alice-1154",
      kind,
      stableId: `instagram:instagram-alice-1154:${kind}:fixture-1154`,
      personName: kind === "comment" ? "Post audience" : "Public audience",
      placeName: `Instagram ${kind.replaceAll("_", " ")} composer`,
      allowed: true,
    },
    composer: { role: "textbox", label, warningState },
  };
}

const completeFixture: InstagramSurfaceFixture = {
  name: "public-post-and-comment",
  composers: [
    composer("public_post", "Write a caption...", "public"),
    composer("comment", "Add a comment...", "public"),
    composer("direct_message", "Message...", "private"),
    composer("group_chat", "Message...", "private"),
    composer("story", "Add text...", "public"),
  ],
};

const fixtureWithoutPublicComposers: InstagramSurfaceFixture = {
  name: "without-public-post-or-comment",
  composers: [
    composer("direct_message", "Message...", "private"),
    composer("group_chat", "Message...", "private"),
  ],
};

function inspectAllowedInstagramComposer(
  candidate: InstagramComposerFixture,
): InstagramComposerInspection {
  if (candidate.place.app !== "instagram"
    || !candidate.place.allowed
    || candidate.composer.role !== "textbox") {
    throw new Error(`TASK1154 ${candidate.place.placeName} is not an allowed Instagram composer`);
  }

  const expectedSurface: Record<InstagramFixtureKind, Readonly<{
    label: string;
    warningState: WarningState;
  }>> = {
    public_post: { label: "Write a caption...", warningState: "public" },
    comment: { label: "Add a comment...", warningState: "public" },
    direct_message: { label: "Message...", warningState: "private" },
    group_chat: { label: "Message...", warningState: "private" },
    story: { label: "Add text...", warningState: "public" },
  };
  const kind = candidate.place.kind as InstagramFixtureKind;
  const expected = expectedSurface[kind];
  if (expected === undefined
    || candidate.composer.label !== expected.label
    || candidate.composer.warningState !== expected.warningState) {
    throw new Error(`TASK1154 ${candidate.place.placeName} composer markers do not match ${candidate.place.kind}`);
  }

  return { kind, warningState: candidate.composer.warningState };
}

function inspectAllowedInstagramPublicComposers(
  fixture: InstagramSurfaceFixture,
): readonly [
  Readonly<{ kind: "public_post"; warningState: "public" }>,
  Readonly<{ kind: "comment"; warningState: "public" }>,
] {
  const publicPost = fixture.composers.find(({ place }) => place.kind === "public_post");
  const comment = fixture.composers.find(({ place }) => place.kind === "comment");
  if (publicPost === undefined || comment === undefined) {
    throw new Error(`TASK1154 ${fixture.name} must contain a public post and comment composer`);
  }

  const publicPostInspection = inspectAllowedInstagramComposer(publicPost);
  const commentInspection = inspectAllowedInstagramComposer(comment);
  if (publicPostInspection.kind !== "public_post" || publicPostInspection.warningState !== "public") {
    throw new Error("TASK1154 public post composer did not return public_post/public");
  }
  if (commentInspection.kind !== "comment" || commentInspection.warningState !== "public") {
    throw new Error("TASK1154 comment composer did not return comment/public");
  }

  return [
    { kind: publicPostInspection.kind, warningState: publicPostInspection.warningState },
    { kind: commentInspection.kind, warningState: commentInspection.warningState },
  ];
}

describe("TASK1154 Instagram public composer inspection", () => {
  it("returns the public post and comment kinds with public warnings", () => {
    const [publicPost, comment] = inspectAllowedInstagramPublicComposers(completeFixture);

    expect(publicPost).toEqual({ kind: "public_post", warningState: "public" });
    expect(comment).toEqual({ kind: "comment", warningState: "public" });
    console.log(`TASK1154_PUBLIC_POST_KIND=${publicPost.kind} TASK1154_PUBLIC_POST_WARNING_STATE=${publicPost.warningState}`);
    console.log(`TASK1154_COMMENT_KIND=${comment.kind} TASK1154_COMMENT_WARNING_STATE=${comment.warningState}`);
  });

  it("rejects a fixture without a public post or comment composer", () => {
    expect(() => inspectAllowedInstagramPublicComposers(fixtureWithoutPublicComposers)).toThrow(
      "TASK1154 without-public-post-or-comment must contain a public post and comment composer",
    );
    console.log("TASK1154_MISSING_PUBLIC_COMPOSERS_REJECTED=true");
  });

  it("runs the selected public-composer fixture check", () => {
    const selected = process.env.TASK1154_CHECK_FIXTURE === "without-public-composers"
      ? fixtureWithoutPublicComposers
      : completeFixture;
    const inspections = inspectAllowedInstagramPublicComposers(selected);

    expect(inspections).toEqual([
      { kind: "public_post", warningState: "public" },
      { kind: "comment", warningState: "public" },
    ]);
    console.log(`TASK1154_SELECTED_FIXTURE=${selected.name} TASK1154_SELECTED_COMPOSERS=${inspections.length}`);
  });
});
