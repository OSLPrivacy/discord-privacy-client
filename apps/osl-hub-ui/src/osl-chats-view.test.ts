import { describe, expect, it } from "vitest";
import { OSL_CHAT_MAX_DRAFT_BYTES, oslChatDraftBytes, oslChatsViewMarkup, type OslChatFriend, type OslChatsViewModel } from "./osl-chats-view";

const friend = (overrides: Partial<OslChatFriend> = {}): OslChatFriend => ({ personId: "friend-1", nickname: "Rose", verified: true, ready: true, preview: "See you soon", previewVisible: true, unreadCount: 0, muted: false, ...overrides });
const model = (overrides: Partial<OslChatsViewModel> = {}): OslChatsViewModel => ({ friends: [friend()], activePersonId: "friend-1", messages: [], draft: "", busy: false, ...overrides });

describe("OSL chats view", () => {
  it("renders standalone direct chat hooks, unread state, and right-click context target", () => {
    const markup = oslChatsViewMarkup(model({ friends: [friend({ unreadCount: 4, muted: true })], homeLogoUrl: "asset://osl.svg" }));
    expect(markup).toContain('data-osl-chat-open="friend-1"');
    expect(markup).toContain('data-osl-chat-settings="friend-1"');
    expect(markup).toContain('data-osl-chat-context="friend-1"');
    expect(markup).toContain('aria-label="4 unread"');
    expect(markup).toContain('aria-label="Muted"');
    expect(markup).not.toMatch(/attachment|Discord|server|group/iu);
  });

  it("escapes every friend and message plaintext boundary", () => {
    const unsafe = '<img src=x onerror="alert(1)">';
    const markup = oslChatsViewMarkup(model({ friends: [friend({ personId: "friend<'1", nickname: unsafe, preview: unsafe })], activePersonId: "friend<'1", messages: [{ messageId: "m<'1", direction: "incoming", body: unsafe, state: "received", timestampLabel: unsafe }], draft: unsafe }));
    expect(markup).not.toContain("<img src=x");
    expect(markup).toContain("&lt;img src=x onerror=&quot;alert(1)&quot;&gt;");
  });

  it("keeps view-once semantics explicit and enforces the UTF-8 send bound", () => {
    expect(oslChatDraftBytes("🔐")).toBe(4);
    const markup = oslChatsViewMarkup(model({ viewOnce: true, draft: "a".repeat(OSL_CHAT_MAX_DRAFT_BYTES + 1) }));
    expect(markup).toContain("Removed from the relay when opened");
    expect(markup).toContain("never added to OSL history");
    expect(markup).toContain("1,001 / 1,000");
    expect(markup).toMatch(/class="osl-chat-send"[^>]*disabled/u);
  });
});
