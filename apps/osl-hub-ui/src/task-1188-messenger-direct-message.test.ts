import { describe, expect, it } from "vitest";
import {
  messengerWhitelistControlsMarkup,
  type MessengerAllowedPlace,
  type MessengerVerificationState,
} from "./messenger-whitelist-controls";

type MessengerFixture = {
  id: "direct-message" | "group-chat" | "community";
  place: MessengerAllowedPlace;
};

const reciprocal: MessengerVerificationState = {
  app: "messenger",
  kind: "direct_message",
  firstAccount: "messenger-alice-1188",
  secondAccount: "messenger-bob-1188",
  firstToSecondAllowed: true,
  secondToFirstAllowed: true,
  state: "two-way",
  verificationTicked: true,
};

const fixture = (id: MessengerFixture["id"], kind: string): MessengerFixture => ({
  id,
  place: {
    app: "messenger",
    account: "messenger-alice-1188",
    kind,
    stableId: `messenger:messenger-alice-1188:${kind}:${id}`,
    personName: "Bob",
    placeName: `TASK1188 ${id}`,
    allowed: true,
  },
});

const fixtures: readonly MessengerFixture[] = [
  fixture("direct-message", "direct_message"),
  fixture("group-chat", "group_chat"),
  fixture("community", "community"),
];

function inspectMessengerFixture(subject: MessengerFixture): { kind: string; controls: string } {
  return {
    kind: subject.place.kind.replace(/_/gu, "-"),
    controls: messengerWhitelistControlsMarkup(subject.place, reciprocal),
  };
}

function selectedFixture(): MessengerFixture {
  const id = process.env.TASK_1188_FIXTURE ?? "direct-message";
  const subject = fixtures.find((candidate) => candidate.id === id);
  if (!subject) throw new Error(`unknown TASK_1188_FIXTURE: ${id}`);
  return subject;
}

describe("TASK1188 Messenger direct-message inspection", () => {
  it("returns exactly direct-message kind and direct-message controls", () => {
    const inspected = inspectMessengerFixture(selectedFixture());

    expect(inspected.kind).toBe("direct-message");
    expect(inspected.controls).toContain("data-messenger-whitelist-controls");
    expect(inspected.controls).toContain("data-messenger-whitelist-toggle");

    console.log(`TASK1188 selected_kind=${inspected.kind} direct_message_controls=${(inspected.controls.match(/data-messenger-whitelist-controls/gu) ?? []).length}`);
  });

  it("keeps group-chat distinct and never classifies another fixture as direct-message", () => {
    const inspected = fixtures.map((subject) => ({ id: subject.id, ...inspectMessengerFixture(subject) }));
    const groupChat = inspected.find(({ id }) => id === "group-chat")!;
    const directMessageCount = inspected.filter(({ kind }) => kind === "direct-message").length;

    expect(groupChat.kind).toBe("group-chat");
    expect(groupChat.kind).not.toBe("direct-message");
    expect(groupChat.controls).toBe("");
    expect(directMessageCount).toBe(1);

    console.log(`TASK1188 group_chat_kind=${groupChat.kind} group_chat_controls=${(groupChat.controls.match(/data-messenger-whitelist-controls/gu) ?? []).length} direct_message_fixture_count=${directMessageCount}`);
  });
});
