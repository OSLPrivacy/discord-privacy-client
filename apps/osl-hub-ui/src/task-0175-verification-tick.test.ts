import { describe, expect, it } from "vitest";
import { blankPeerProtectedModel, peerProtectedSheetMarkup, type PeerProtectedSheetModel } from "./peer-protected-sheet";
import { verificationTickMarkup, type AllowedPlaceDirectionStateModel } from "./verification-tick";

// Paired fixtures: the same conversation with the same person, differing only
// in the compared whitelist directions the backend reported.
const oneWayDirectionState: AllowedPlaceDirectionStateModel = {
  state: "one-way",
  verificationState: "hidden",
  savedDirections: 1,
  firstToSecond: true,
  secondToFirst: false,
};

const twoWayDirectionState: AllowedPlaceDirectionStateModel = {
  state: "two-way",
  verificationState: "visible",
  savedDirections: 2,
  firstToSecond: true,
  secondToFirst: true,
};

function conversationFixture(directionState: AllowedPlaceDirectionStateModel | null): PeerProtectedSheetModel {
  const model = blankPeerProtectedModel(true);
  model.context = {
    contextToken: "ctx-task-0175",
    serviceId: "discord",
    accountId: "900000000000000175",
    personId: "person-ada",
    peerOslUserId: "osl-user-ada",
    scopeApproved: true,
  };
  model.personId = "person-ada";
  model.displayName = "Ada Lovelace";
  model.directionState = directionState;
  return model;
}

function tickCount(markup: string): number {
  return [...markup.matchAll(/data-verification-tick="two-way"/gu)].length;
}

describe("TASK0175 verification tick", () => {
  it("shows the tick only in the two-way conversation fixture", () => {
    const oneWayMarkup = peerProtectedSheetMarkup(conversationFixture(oneWayDirectionState), []);
    const twoWayMarkup = peerProtectedSheetMarkup(conversationFixture(twoWayDirectionState), []);

    const oneWayTicks = tickCount(oneWayMarkup);
    const twoWayTicks = tickCount(twoWayMarkup);

    console.log(`TASK0175 fixture=one-way state=${oneWayDirectionState.state} verification_state=${oneWayDirectionState.verificationState} ticks=${oneWayTicks}`);
    console.log(`TASK0175 fixture=two-way state=${twoWayDirectionState.state} verification_state=${twoWayDirectionState.verificationState} ticks=${twoWayTicks}`);

    // Both fixtures render the same conversation with the same person…
    expect(oneWayMarkup).toContain("Ada Lovelace");
    expect(twoWayMarkup).toContain("Ada Lovelace");
    // …but only the two-way fixture carries the tick, beside the person's name.
    expect(oneWayTicks).toBe(0);
    expect(twoWayTicks).toBe(1);
    expect(twoWayMarkup).toContain(`<h2 id="peer-protected-title">Ada Lovelace<span class="verification-tick" data-verification-tick="two-way" role="img" aria-label="Verified both ways">✓</span></h2>`);
    expect(oneWayMarkup).toContain(`<h2 id="peer-protected-title">Ada Lovelace</h2>`);
  });

  it("draws nothing for none, unloaded, or self-contradicting direction states", () => {
    const none: AllowedPlaceDirectionStateModel = {
      state: "none",
      verificationState: "hidden",
      savedDirections: 0,
      firstToSecond: false,
      secondToFirst: false,
    };
    // A report claiming two-way while the gate says hidden must fail closed.
    const contradicting: AllowedPlaceDirectionStateModel = { ...twoWayDirectionState, verificationState: "hidden" };

    expect(verificationTickMarkup(none)).toBe("");
    expect(verificationTickMarkup(null)).toBe("");
    expect(verificationTickMarkup(contradicting)).toBe("");
    expect(verificationTickMarkup(oneWayDirectionState)).toBe("");
    expect(tickCount(peerProtectedSheetMarkup(conversationFixture(null), []))).toBe(0);
  });
});
