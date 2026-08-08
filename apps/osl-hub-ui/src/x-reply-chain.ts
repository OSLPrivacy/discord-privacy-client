import type { XAllowedPlace } from "./x-whitelist-controls";

/** The reply composer found in the currently inspected X conversation chain. */
export interface XReplyComposer {
  readonly kind: "reply";
  readonly accessibleName: string;
}

/**
 * A prepared X surface record. Parent-post and reply permission are kept as
 * separate facts so a post allowance can never silently authorize a reply.
 */
export interface XReplyChainFixture {
  readonly replyPlace: XAllowedPlace;
  readonly parentPostAllowed: boolean;
  readonly composer: XReplyComposer | null;
}

export interface XReplyComposerInspection {
  readonly kind: "reply";
  readonly composer: string;
  readonly postAllowanceInherited: false;
}

/**
 * Inspect an explicitly allowed X reply and return only its reply composer.
 *
 * The parent post's allowance is intentionally not part of the authorization
 * decision. `replyPlace.allowed` is the distinct permission for this reply.
 */
export function inspectAllowedXReplyComposer(
  fixture: XReplyChainFixture,
): XReplyComposerInspection {
  const { replyPlace, composer } = fixture;
  if (replyPlace.app !== "x" || replyPlace.kind !== "reply") {
    throw new Error("TASK1127 expected an X reply fixture");
  }
  if (!replyPlace.allowed) {
    throw new Error("TASK1127 explicit reply permission is required");
  }
  if (composer === null || composer.kind !== "reply" || composer.accessibleName.trim() === "") {
    throw new Error("TASK1127 allowed X reply fixture has no reply composer");
  }

  return {
    kind: "reply",
    composer: composer.accessibleName,
    postAllowanceInherited: false,
  };
}
