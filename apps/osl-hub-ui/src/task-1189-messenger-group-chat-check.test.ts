import { describe, expect, it } from "vitest";

type MessengerFixture = {
  id: string;
  app: "messenger" | "instagram" | "signal";
  allowed: boolean;
  kind: "direct_message" | "group_chat" | "community";
  controls: readonly string[];
};

type GroupChatInspection = {
  kind: "group_chat";
  controls: readonly ["allow_group_chat", "remove_group_chat", "group_chat_verification"];
};

const GROUP_CHAT_CONTROLS = [
  "allow_group_chat",
  "remove_group_chat",
  "group_chat_verification",
] as const;

const fixtures: readonly MessengerFixture[] = [
  {
    id: "allowed-messenger-group-chat",
    app: "messenger",
    allowed: true,
    kind: "group_chat",
    controls: GROUP_CHAT_CONTROLS,
  },
  {
    id: "allowed-messenger-direct-message",
    app: "messenger",
    allowed: true,
    kind: "direct_message",
    controls: ["allow_direct_message", "remove_direct_message", "direct_message_verification"],
  },
  {
    id: "unallowed-messenger-community",
    app: "messenger",
    allowed: false,
    kind: "community",
    controls: [],
  },
  {
    id: "allowed-signal-group-chat",
    app: "signal",
    allowed: true,
    kind: "group_chat",
    controls: ["allow_group_chat"],
  },
  {
    id: "allowed-instagram-direct-message",
    app: "instagram",
    allowed: true,
    kind: "direct_message",
    controls: ["allow_direct_message"],
  },
];

function inspectAllowedMessengerGroupChat(fixture: MessengerFixture): GroupChatInspection {
  if (fixture.app !== "messenger" || !fixture.allowed || fixture.kind !== "group_chat") {
    throw new Error(`TASK1189 expected an allowed Messenger group chat, got ${fixture.app}:${fixture.kind}`);
  }
  if (fixture.controls.join(",") !== GROUP_CHAT_CONTROLS.join(",")) {
    throw new Error("TASK1189 group-chat controls do not match the canonical controls");
  }
  return { kind: fixture.kind, controls: GROUP_CHAT_CONTROLS };
}

describe("TASK1189 Messenger group-chat check", () => {
  it("returns exactly the allowed Messenger group-chat kind and controls", () => {
    const groupChat = fixtures.find(({ id }) => id === "allowed-messenger-group-chat");
    expect(groupChat).toBeDefined();

    const inspection = inspectAllowedMessengerGroupChat(groupChat!);
    expect(inspection).toEqual({ kind: "group_chat", controls: GROUP_CHAT_CONTROLS });

    console.log(`TASK1189_GROUP_KIND=${inspection.kind}`);
    console.log(`TASK1189_GROUP_CONTROLS=${inspection.controls.join(",")}`);
  });

  it("keeps direct messages distinct and never reports another fixture as the Messenger group chat", () => {
    const directMessage = fixtures.find(({ id }) => id === "allowed-messenger-direct-message");
    expect(directMessage).toBeDefined();
    expect(directMessage!.kind).toBe("direct_message");
    expect(directMessage!.kind).not.toBe("group_chat");

    const otherGroupChatCount = fixtures
      .filter(({ id }) => id !== "allowed-messenger-group-chat")
      .filter((fixture) => {
        try {
          inspectAllowedMessengerGroupChat(fixture);
          return true;
        } catch {
          return false;
        }
      })
      .length;
    expect(otherGroupChatCount).toBe(0);

    console.log(`TASK1189_DIRECT_KIND=${directMessage!.kind}`);
    console.log(`TASK1189_OTHER_FIXTURE_GROUP_CHAT_COUNT=${otherGroupChatCount}`);
  });

  it("fails when deliberately pointed at the direct-message fixture", () => {
    const directMessage = fixtures.find(({ id }) => id === "allowed-messenger-direct-message");
    expect(directMessage).toBeDefined();

    expect(() => inspectAllowedMessengerGroupChat(directMessage!)).toThrow(
      "TASK1189 expected an allowed Messenger group chat, got messenger:direct_message",
    );
    console.log("TASK1189_DIRECT_MESSAGE_NEGATIVE_CONTROL=failed_as_expected");
  });
});
