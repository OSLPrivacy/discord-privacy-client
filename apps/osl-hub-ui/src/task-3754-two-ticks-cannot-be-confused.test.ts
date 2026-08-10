import { describe, expect, it } from "vitest";
import {
  oslChatsViewMarkup,
  type OslChatFriend,
  type OslChatsViewModel,
} from "./osl-chats-view";

const PROOF_NAME = "FRIEND-LAPTOP-3754.proof.json";
const CONSENT_WORDS = "Both people allow each other";
const BUILD_WORDS = `Unmodified build — ${PROOF_NAME}`;

const friend: OslChatFriend = {
  personId: "friend-3754",
  nickname: "Maya Chen",
  verified: true,
  ready: true,
  preview: "Two facts, two marks",
  previewVisible: true,
  unreadCount: 0,
  verificationTwoWay: true,
  buildProof: {
    proofName: PROOF_NAME,
    answer: "unmodified",
  },
};

function renderFriend(overrides: Partial<OslChatFriend> = {}): string {
  const model: OslChatsViewModel = {
    friends: [{ ...friend, ...overrides }],
    // Keep the friend inactive so this assertion measures the one friend row,
    // without a second copy of its marks in the active thread header.
    activePersonId: null,
    messages: [],
    draft: "",
    busy: false,
  };
  return oslChatsViewMarkup(model);
}

function markWords(markup: string): string[] {
  return [...markup.matchAll(
    /<span(?=[^>]*data-osl-(?:consent-mark|build-mark)=)[^>]*aria-label="([^"]+)"[^>]*>/gu,
  )].map((match) => match[1]);
}

describe("TASK 3754 the two friend-row ticks cannot be confused", () => {
  it("shows both independent words, then removes each mark independently", () => {
    const both = markWords(renderFriend());
    expect(both).toEqual([CONSENT_WORDS, BUILD_WORDS]);
    expect(both).toHaveLength(2);
    expect(new Set(both).size).toBe(2);

    const buildRemoved = markWords(renderFriend({ buildProof: undefined }));
    expect(buildRemoved).toEqual([CONSENT_WORDS]);
    expect(buildRemoved).toHaveLength(1);
    expect(buildRemoved.join(" ")).not.toMatch(/unmodified/iu);

    const consentRemoved = markWords(renderFriend({ verificationTwoWay: false }));
    expect(consentRemoved).toEqual([BUILD_WORDS]);
    expect(consentRemoved).toHaveLength(1);
    expect(consentRemoved.join(" ")).not.toMatch(/allowed/iu);

    console.log(
      `TASK3754_BOTH mark_count=${both.length} distinct_word_count=${new Set(both).size} words=${JSON.stringify(both)}`,
    );
    console.log(
      `TASK3754_BUILD_REMOVED mark_count=${buildRemoved.length} words=${JSON.stringify(buildRemoved)} contains_unmodified=${/unmodified/iu.test(buildRemoved.join(" "))}`,
    );
    console.log(
      `TASK3754_CONSENT_REMOVED mark_count=${consentRemoved.length} words=${JSON.stringify(consentRemoved)} contains_allowed=${/allowed/iu.test(consentRemoved.join(" "))}`,
    );
  });
});
