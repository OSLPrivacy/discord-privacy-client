import { describe, expect, it } from "vitest";
import {
  inspectAllowedXReplyComposer,
  type XReplyChainFixture,
} from "./x-reply-chain";

function fixture(
  options: Readonly<{ replyAllowed?: boolean; withReplyComposer?: boolean }> = {},
): XReplyChainFixture {
  const replyAllowed = options.replyAllowed ?? true;
  const withReplyComposer = options.withReplyComposer ?? true;
  return {
    replyPlace: {
      app: "x",
      account: "x-alice-1127",
      kind: "reply",
      stableId: "x:x-alice-1127:reply:post-1127",
      personName: "Bob",
      placeName: "Reply to Bob's post",
      allowed: replyAllowed,
    },
    // Deliberately true even in the no-reply-permission case. The check must
    // use replyPlace.allowed rather than borrowing this parent permission.
    parentPostAllowed: true,
    composer: withReplyComposer
      ? { kind: "reply", accessibleName: "Post your reply" }
      : null,
  };
}

describe("TASK1127 X reply-chain inspection", () => {
  it("directly returns reply kind from the allowed reply composer", () => {
    const inspection = inspectAllowedXReplyComposer(fixture());

    expect(inspection).toEqual({
      kind: "reply",
      composer: "Post your reply",
      postAllowanceInherited: false,
    });
    console.log(
      `TASK1127_REPLY_KIND=${inspection.kind} TASK1127_REPLY_COMPOSER=${inspection.composer} TASK1127_POST_ALLOWANCE_INHERITED=${inspection.postAllowanceInherited}`,
    );
  });

  it("refuses to inherit an allowed parent post without reply permission", () => {
    const postOnly = fixture({ replyAllowed: false });

    expect(postOnly.parentPostAllowed).toBe(true);
    expect(() => inspectAllowedXReplyComposer(postOnly)).toThrow(
      "TASK1127 explicit reply permission is required",
    );
    console.log("TASK1127_POST_ALLOWED=true TASK1127_REPLY_ALLOWED=false TASK1127_POST_ALLOWANCE_INHERITED=false");
  });

  it("fails when an allowed reply fixture has no reply composer", () => {
    expect(() => inspectAllowedXReplyComposer(fixture({ withReplyComposer: false }))).toThrow(
      "TASK1127 allowed X reply fixture has no reply composer",
    );
    console.log("TASK1127_MISSING_REPLY_COMPOSER_REJECTED=true");
  });

  it("runs the selected reply-composer check", () => {
    const selected = process.env.TASK1127_CHECK_FIXTURE === "without_reply_composer"
      ? fixture({ withReplyComposer: false })
      : fixture();
    const inspection = inspectAllowedXReplyComposer(selected);
    expect(inspection.kind).toBe("reply");
    console.log(`TASK1127_SELECTED_KIND=${inspection.kind}`);
  });
});
