import { describe, expect, it } from "vitest";
import {
  OSL_CHAT_DELIVERY_STATES,
  OSL_CHAT_MAX_DRAFT_BYTES,
  oslChatDraftBytes,
  oslChatsViewMarkup,
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
    const markup = oslChatsViewMarkup(model({
      messages: OSL_CHAT_DELIVERY_STATES.map((state) => ({ messageId: state, direction: state === "received" ? "incoming" : "outgoing", body: state, state, timestampLabel: "Now" })),
    }));
    expect(OSL_CHAT_DELIVERY_STATES).toEqual(["sent", "delivered", "received", "opened", "expired", "failed"]);
    for (const state of OSL_CHAT_DELIVERY_STATES) {
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

  it("offers view-once with exact open-and-history semantics", () => {
    const markup = oslChatsViewMarkup(model({ viewOnce: true }));
    expect(markup).toContain('id="osl-chat-view-once"');
    expect(markup).toContain("Removed after it is opened");
    expect(markup).toContain("kept out of OSL history");
    expect(markup).toMatch(/id="osl-chat-view-once" type="checkbox" checked/u);
  });

  it("enables send only for a verified ready friend with a valid nonempty draft", () => {
    expect(oslChatsViewMarkup(model({ draft: "Hello" }))).toMatch(/class="osl-chat-send" type="submit"(?![^>]* disabled)[^>]*>/u);
    expect(oslChatsViewMarkup(model({ draft: "Hello", friends: [friend({ verified: false })] }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(oslChatsViewMarkup(model({ draft: "Hello", friends: [friend({ ready: false })] }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(oslChatsViewMarkup(model({ draft: "   " }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(oslChatsViewMarkup(model({ draft: "Hello", busy: true }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
  });

  it("accepts the backend maximum and blocks the first draft the backend would reject", () => {
    expect(oslChatDraftBytes("🔐")).toBe(4);
    const backendMaximum = 1024 * 1024;
    const accepted = oslChatsViewMarkup(model({ draft: "a".repeat(backendMaximum) }));
    expect(OSL_CHAT_MAX_DRAFT_BYTES).toBe(backendMaximum);
    expect(accepted).toContain("1,048,576 / 1,048,576");
    expect(accepted).toMatch(/class="osl-chat-send" type="submit"(?![^>]* disabled)[^>]*>/u);

    const refused = oslChatsViewMarkup(model({ draft: "a".repeat(backendMaximum + 1) }));
    expect(refused).toContain("osl-chat-byte-count is-over");
    expect(refused).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
  });

  it("renders no external history, scripts, or backend capability claims", () => {
    const markup = oslChatsViewMarkup(model());
    expect(markup).not.toContain("<script");
    expect(markup).not.toMatch(/Discord|Signal|Telegram|Snapchat|encrypted|end-to-end|server|group|keyserver|ratchet|receipt|browser profile|provider adapter|relay/iu);
  });
});

describe("send button re-enablement while typing", () => {
  it("marks the send context ready when only the empty draft blocks sending", () => {
    // The Send button's disabled state is computed at RENDER time, but typing
    // does not re-render (a re-render would rebuild the textarea under the
    // caret). So the button carries the preconditions that CANNOT change while
    // typing, and the input handler re-evaluates only draft-dependent ones.
    // Without this, a user typed a message and Send stayed dead until some
    // unrelated event repainted -- measured on a real VM.
    const markup = oslChatsViewMarkup(model({ draft: "" }));
    expect(markup).toContain('data-osl-chat-send-context="1"');
    // ...and it is genuinely disabled right now, because the draft is empty.
    expect(markup).toMatch(/data-osl-chat-send-context="1"[^>]*disabled/u);
  });

  it("marks the send context NOT ready when a precondition typing cannot fix is unmet", () => {
    // These three cannot be cleared by typing, so the input handler must never
    // enable Send on their account.
    for (const [label, m] of [
      ["unverified", model({ friends: [friend({ verified: false })], draft: "hello" })],
      ["not ready", model({ friends: [friend({ ready: false })], draft: "hello" })],
      ["busy", model({ draft: "hello", busy: true })],
    ] as const) {
      expect(oslChatsViewMarkup(m), label).toContain('data-osl-chat-send-context="0"');
    }
  });

  it("enables send once a draft exists and every other precondition holds", () => {
    const markup = oslChatsViewMarkup(model({ draft: "hello" }));
    expect(markup).toContain('data-osl-chat-send-context="1"');
    expect(markup).not.toMatch(/class="osl-chat-send"[^>]*disabled/u);
  });
});
