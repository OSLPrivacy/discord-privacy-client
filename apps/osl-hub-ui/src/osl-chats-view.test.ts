import { describe, expect, it } from "vitest";
import {
  OSL_CHAT_MAX_DRAFT_BYTES,
  oslChatDraftBytes,
  oslChatsViewMarkup,
  type OslChatDeliveryState,
  type OslChatFriend,
  type OslChatsViewModel,
} from "./osl-chats-view";

function friend(overrides: Partial<OslChatFriend> = {}): OslChatFriend {
  return {
    personId: "friend-1",
    nickname: "Rose",
    verified: true,
    ready: true,
    preview: "See you soon",
    previewVisible: true,
    unreadCount: 0,
    muted: false,
    ...overrides,
  };
}

function model(overrides: Partial<OslChatsViewModel> = {}): OslChatsViewModel {
  return {
    friends: [friend()],
    activePersonId: "friend-1",
    messages: [],
    draft: "",
    busy: false,
    ...overrides,
  };
}

describe("OSL chats view", () => {
  it("renders a friend-first direct-message view with separate open and settings hooks", () => {
    const markup = oslChatsViewMarkup(model({ homeLogoUrl: "asset://osl-logo.svg" }));
    expect(markup).toContain('data-osl-chat-open="friend-1"');
    expect(markup).toContain('data-osl-chat-settings="friend-1"');
    expect(markup).toContain('class="osl-chat-home" data-route="home"');
    expect(markup).toContain('src="asset://osl-logo.svg"');
    expect(markup).toContain('aria-label="OSL direct chat with Rose"');
    expect(markup).not.toMatch(/server|group/iu);
  });

  it("escapes nicknames, previews, IDs, timestamps, messages, and drafts", () => {
    const unsafe = '<img src=x onerror="alert(1)">';
    const markup = oslChatsViewMarkup(model({
      friends: [friend({ personId: "friend<'1", nickname: unsafe, preview: unsafe })],
      activePersonId: "friend<'1",
      messages: [{ messageId: "m<'1", direction: "incoming", body: unsafe, state: "received", timestampLabel: unsafe }],
      draft: unsafe,
    }));
    expect(markup).not.toContain("<img");
    expect(markup).not.toContain("onerror=\"");
    expect(markup).toContain("&lt;img src=x onerror=&quot;alert(1)&quot;&gt;");
    expect(markup).toContain('data-person-id="friend&lt;&#39;1"');
  });

  it("preserves exact multiline message and draft text in white-space-safe elements", () => {
    const body = "one\n\nthree\nlast";
    const markup = oslChatsViewMarkup(model({
      messages: [{ messageId: "m1", direction: "outgoing", body, state: "delivered", timestampLabel: "Now" }],
      draft: "draft one\n\ndraft three",
    }));
    expect(markup).toContain(`<p class="osl-chat-message-text">${body}</p>`);
    expect(markup).toContain("draft one\n\ndraft three</textarea>");
    expect(markup).toContain("osl-chat-message-text");
  });

  it("shows every honest delivery tag without inferring another state", () => {
    const states: readonly OslChatDeliveryState[] = ["sent", "delivered", "received", "opened", "expired", "failed"];
    const markup = oslChatsViewMarkup(model({
      messages: states.map((state) => ({ messageId: state, direction: state === "received" ? "incoming" : "outgoing", body: state, state, timestampLabel: "Now" })),
    }));
    for (const state of states) {
      const label = state[0].toUpperCase() + state.slice(1);
      expect(markup).toContain(`class="osl-chat-message-state is-${state}">${label}</span>`);
    }
  });

  it("uses a neutral hidden-preview state and an honest empty-preview state", () => {
    const hidden = oslChatsViewMarkup(model({ friends: [friend({ previewVisible: false })] }));
    expect(hidden).toContain("Preview hidden");
    expect(hidden).not.toContain("See you soon");
    const empty = oslChatsViewMarkup(model({ friends: [friend({ preview: null })] }));
    expect(empty).toContain("No messages yet");
  });

  it("offers view-once with exact relay and history semantics", () => {
    const markup = oslChatsViewMarkup(model({ viewOnce: true }));
    expect(markup).toContain('id="osl-chat-view-once"');
    expect(markup).toContain("Removed from the relay when opened");
    expect(markup).toContain("never added to OSL history");
    expect(markup).toMatch(/id="osl-chat-view-once" type="checkbox" checked/u);
  });

  it("enables send only for a verified ready friend with a valid nonempty draft", () => {
    expect(oslChatsViewMarkup(model({ draft: "Hello" }))).toMatch(/class="osl-chat-send" type="submit"(?![^>]* disabled)[^>]*>/u);
    expect(oslChatsViewMarkup(model({ draft: "Hello", friends: [friend({ verified: false })] }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(oslChatsViewMarkup(model({ draft: "Hello", friends: [friend({ ready: false })] }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(oslChatsViewMarkup(model({ draft: "   " }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(oslChatsViewMarkup(model({ draft: "Hello", busy: true }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
  });

  it("counts UTF-8 bytes and rejects over-limit drafts", () => {
    expect(oslChatDraftBytes("🔐")).toBe(4);
    const draft = "a".repeat(OSL_CHAT_MAX_DRAFT_BYTES + 1);
    const markup = oslChatsViewMarkup(model({ draft }));
    expect(markup).toContain("1,001 / 1,000");
    expect(markup).toContain("osl-chat-byte-count is-over");
    expect(markup).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
  });

  it("renders no external history, scripts, or backend capability claims", () => {
    const markup = oslChatsViewMarkup(model());
    expect(markup).not.toContain("<script");
    expect(markup).not.toMatch(/Discord|Signal|Telegram|Snapchat|encrypted|end-to-end|server|group/iu);
  });

  it("suppresses every inline message and compose link preview when disabled", () => {
    const withLinks = model({
      messages: [{ messageId: "m-link", direction: "incoming", body: "See https://example.org/a", state: "received", timestampLabel: "Now" }],
      draft: "Draft https://example.net/b",
      disableLinkPreviews: true,
    });
    const hidden = oslChatsViewMarkup(withLinks);
    expect(hidden).not.toContain("osl-chat-link-preview");
    expect(hidden).toContain("osl-chat-external-link");
    expect(hidden).toContain('data-external-url="https://example.org/a"');
    expect(hidden).not.toContain("<strong>example.org</strong>");

    const shown = oslChatsViewMarkup({ ...withLinks, disableLinkPreviews: false });
    expect(shown).toContain('data-external-url="https://example.org/a"');
    expect(shown).toContain('data-external-url="https://example.net/b"');
    expect(shown).toContain("Open in browser");
  });

  it("renders unread count, muted state, and a right-click context target per friend row", () => {
    const markup = oslChatsViewMarkup(model({ friends: [friend({ unreadCount: 4, muted: true })] }));
    expect(markup).toContain('data-osl-chat-context="friend-1"');
    expect(markup).toContain('aria-label="4 unread"');
    expect(markup).toContain('aria-label="Muted"');
  });
});
