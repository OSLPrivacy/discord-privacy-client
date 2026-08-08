import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  oslChatsViewMarkup,
  oslVerificationTickMarkup,
  type OslChatFriend,
  type OslChatsViewModel,
} from "./osl-chats-view";

const TWO_WAY_FIXTURE = path.join(
  import.meta.dirname,
  "..",
  "screenshots",
  "fixtures",
  "task-0175-verification-tick-two-way.json",
);
const ONE_WAY_FIXTURE = path.join(
  import.meta.dirname,
  "..",
  "screenshots",
  "fixtures",
  "task-0175-verification-tick-one-way.json",
);

function fixture(fixturePath: string): OslChatFriend {
  return JSON.parse(readFileSync(fixturePath, "utf8")) as OslChatFriend;
}

function threadModel(friend: OslChatFriend): OslChatsViewModel {
  return {
    friends: [friend],
    activePersonId: friend.personId,
    messages: [],
    draft: "",
    busy: false,
  };
}

describe("TASK 0175 verification tick", () => {
  it("draws the tick only for the two-way fixture, on both the person row and the conversation header", () => {
    const twoWay = fixture(TWO_WAY_FIXTURE);
    const oneWay = fixture(ONE_WAY_FIXTURE);
    expect(twoWay.verificationTwoWay).toBe(true);
    expect(oneWay.verificationTwoWay).toBe(false);

    const twoWayMarkup = oslChatsViewMarkup(threadModel(twoWay));
    const oneWayMarkup = oslChatsViewMarkup(threadModel(oneWay));

    const twoWayTickCount = twoWayMarkup.split('data-osl-verification-tick="visible"').length - 1;
    const oneWayTickCount = oneWayMarkup.split('data-osl-verification-tick="visible"').length - 1;

    // Person row (friend list) + conversation (thread) header == 2 draws.
    expect(twoWayTickCount).toBe(2);
    expect(oneWayTickCount).toBe(0);
    expect(oneWayMarkup).not.toContain("osl-verification-tick");

    console.log(`TASK0175 fixture=two-way person=${twoWay.nickname} tick_count=${twoWayTickCount}`);
    console.log(`TASK0175 fixture=one-way person=${oneWay.nickname} tick_count=${oneWayTickCount}`);
  });

  it("treats undefined and false the same as one-way: no tick", () => {
    expect(oslVerificationTickMarkup(undefined)).toBe("");
    expect(oslVerificationTickMarkup(false)).toBe("");
    expect(oslVerificationTickMarkup(true)).toContain('data-osl-verification-tick="visible"');
  });
});

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

// Paired fixtures: the same conversation with the same person, differing only
// in the compared whitelist directions the backend reported.
const oneWayDirectionState: AllowedPlaceDirectionStateModel = {
  state: "one-way",
  verificationState: "hidden",
  savedDirections: 1,
  firstToSecond: true,
  secondToFirst: false,
};

function tickCount(markup: string): number {
  return [...markup.matchAll(/data-verification-tick="two-way"/gu)].length;
}

const twoWayDirectionState: AllowedPlaceDirectionStateModel = {
  state: "two-way",
  verificationState: "visible",
  savedDirections: 2,
  firstToSecond: true,
  secondToFirst: true,
};
