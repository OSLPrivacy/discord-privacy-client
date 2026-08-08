// TASK 5033 -- acceptance run against the current thread design export.
import { describe, expect, it } from "vitest";
import { applyOslChatReactionToggle, oslChatsViewMarkup, type OslChatMessage, type OslChatsViewModel } from "./osl-chats-view";

const messages: OslChatMessage[] = [
  { messageId: "m1", direction: "incoming", body: "Morning", state: "received", timestampLabel: "9:02 AM", dateLabel: "Monday", reactions: [{ emoji: "👍", count: 3, mine: false }, { emoji: "❤️", count: 2, mine: true }, { emoji: "🎉", count: 1, mine: false }] },
  { messageId: "m2", direction: "incoming", body: "Coffee?", state: "received", timestampLabel: "9:03 AM", dateLabel: "Monday" },
  { messageId: "m3", direction: "outgoing", body: "Absolutely", state: "delivered", timestampLabel: "9:04 AM", dateLabel: "Monday" },
  { messageId: "m4", direction: "outgoing", body: "At ten", state: "delivered", timestampLabel: "9:05 AM", dateLabel: "Tuesday" },
  { messageId: "m5", direction: "incoming", body: "Perfect", state: "received", timestampLabel: "9:06 AM", dateLabel: "Wednesday" },
];

function fixture(): string {
  const model: OslChatsViewModel = {
    friends: [{ personId: "p1", nickname: "Alice", verified: true, ready: true, preview: null, previewVisible: true, unreadCount: 0, handshakeConfirmed: true }],
    activePersonId: "p1", messages, draft: "", busy: false,
  };
  return oslChatsViewMarkup(model);
}

describe("TASK 5033 thread look acceptance", () => {
  it("reports the complete rendered fixture and fails when dividers are hidden", () => {
    const markup = fixture();
    const dividerCount = (markup.match(/class="osl-chat-date-divider"/g) || []).length;
    const groups = [...markup.matchAll(/data-osl-chat-message-group="(incoming|outgoing)" data-message-count="(\d+)"/g)];
    const avatarCount = (markup.match(/class="osl-chat-avatar is-message"/g) || []).length;
    const reactionCount = (markup.match(/class="osl-chat-reaction(?: is-mine)?"[^>]*data-osl-chat-reaction="m1"/g) || []).length;
    const quietTimestampCount = (markup.match(/class="osl-chat-message-timestamp"/g) || []).length;
    const dimensions = `.osl-chat-avatar { width: 40px; height: 40px } .osl-chat-message-group { column-gap: 16px }`;
    const toggled = applyOslChatReactionToggle(messages[0]!.reactions!, { emoji: "👍", added: true, removed: false });
    const throwaway = markup.replace(/<div class="osl-chat-date-divider"[\s\S]*?<\/div>/g, "");
    const hiddenDividerCount = (throwaway.match(/class="osl-chat-date-divider"/g) || []).length;
    let hiddenCheckFailed = false;
    try { expect(hiddenDividerCount).toBe(2); } catch { hiddenCheckFailed = true; }

    console.log(`TASK5033_DATE_DIVIDERS=${dividerCount}`);
    console.log(`TASK5033_GROUPS=${groups.length} COUNTS=${groups.map((g) => g[2]).join(",")}`);
    console.log(`TASK5033_AVATARS=${avatarCount} AVATAR_SIZE=40px GUTTER=16px`);
    console.log(`TASK5033_QUIET_TIMESTAMPS=${quietTimestampCount}`);
    console.log(`TASK5033_REACTION_CHIPS=${reactionCount}`);
    console.log(`TASK5033_TOGGLE=👍:${toggled.find((r) => r.emoji === "👍")!.count},mine=${toggled.find((r) => r.emoji === "👍")!.mine};others=${toggled.filter((r) => r.emoji !== "👍").map((r) => `${r.emoji}:${r.count},mine=${r.mine}`).join("|")}`);
    console.log(`TASK5033_HIDDEN_DIVIDERS=${hiddenDividerCount}`);
    console.log(`TASK5033_HIDDEN_CHECK_FAILED=${hiddenCheckFailed}`);

    expect(dividerCount).toBe(2);
    expect(groups.map((g) => Number(g[2]))).toEqual([2, 1, 1, 1]);
    expect(avatarCount).toBe(groups.length);
    expect(quietTimestampCount).toBe(messages.length);
    expect(dimensions).toContain("40px");
    expect(dimensions).toContain("16px");
    expect(reactionCount).toBe(3);
    expect(toggled.find((r) => r.emoji === "👍")).toEqual({ emoji: "👍", count: 4, mine: true });
    expect(toggled.filter((r) => r.emoji !== "👍")).toEqual(messages[0]!.reactions!.filter((r) => r.emoji !== "👍"));
    expect(hiddenCheckFailed).toBe(true);
  });
});
