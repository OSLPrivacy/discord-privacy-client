import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  OSL_CHAT_DELIVERY_STATES,
  OSL_CHAT_MAX_DRAFT_BYTES,
  applyOslChatDraftToElement,
  oslChatDraftBytes,
  oslChatsViewMarkup,
  submitsOslChatDraft,
  type OslChatFriend,
  type OslChatsViewModel,
} from "./osl-chats-view";

function friend(overrides: Partial<OslChatFriend> = {}): OslChatFriend {
  return {
    personId: "friend-1",
    nickname: "Rose",
    verified: true,
    ready: true,
    handshakeConfirmed: true,
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
    expect(OSL_CHAT_DELIVERY_STATES).toEqual(["queued", "sent", "delivered", "received", "opened", "expired", "failed"]);
    const labels: Record<typeof OSL_CHAT_DELIVERY_STATES[number], string> = {
      queued: "Not sent",
      sent: "Sent",
      delivered: "Delivered",
      received: "Received",
      opened: "Opened",
      expired: "Expired",
      failed: "Failed",
    };
    for (const state of OSL_CHAT_DELIVERY_STATES) {
      const label = labels[state];
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
    // D-135. This assertion used to be `toContain("Removed after it is
    // opened")`, and that sentence was the defect: OSL asks for the sent copy
    // to be deleted, learns nothing back, and counted the failures into
    // `DeletionDrainReport::retained` where they were discarded. The local half
    // of the promise is real and still pinned; the remote half is now stated as
    // a request, and pinned as NOT a completion. Master 7.5.
    expect(markup).toContain("Kept out of OSL history");
    expect(markup).toContain("asks for the sent copy to be deleted");
    expect(markup).not.toMatch(/\bRemoved after it is opened\b/u);
    expect(markup).toMatch(/id="osl-chat-view-once" type="checkbox" checked/u);
  });

  it("enables send only for a verified ready friend with a valid nonempty draft", () => {
    expect(oslChatsViewMarkup(model({ draft: "Hello" }))).toMatch(/class="osl-chat-send" type="submit"(?![^>]* disabled)[^>]*>/u);
    expect(oslChatsViewMarkup(model({ draft: "Hello", friends: [friend({ verified: false })] }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(oslChatsViewMarkup(model({ draft: "Hello", friends: [friend({ ready: false })] }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(oslChatsViewMarkup(model({ draft: "Hello", friends: [friend({ handshakeConfirmed: false })] }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(oslChatsViewMarkup(model({ draft: "   " }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
    expect(oslChatsViewMarkup(model({ draft: "Hello", busy: true }))).toMatch(/class="osl-chat-send" type="submit"[^>]* disabled/u);
  });

  it("shows changed and corrupt build warnings without blocking message sending", () => {
    for (const [status, detail] of [
      ["mismatch", "This app copy does not match OSL&#39;s signed build list."],
      ["unknown", "OSL could not verify this app copy against its signed build list."],
    ] as const) {
      const markup = oslChatsViewMarkup(model({ draft: "Hello", buildIntegrity: status }));
      expect(markup, status).toContain(`data-osl-build-integrity="${status}"`);
      expect(markup, status).toContain("Build verification warning");
      expect(markup, status).toContain(detail);
      expect(markup, status).toMatch(/data-osl-chat-send-context="1"[^>]*(?<!disabled)>/u);
      expect(markup, status).toMatch(/class="osl-chat-send" type="submit"(?![^>]* disabled)[^>]*>/u);
    }

    expect(oslChatsViewMarkup(model({ draft: "Hello", buildIntegrity: "verified" })))
      .not.toContain("Build verification warning");
  it("TASK0434 starts direct chats only when both peers have answered", () => {
    const supported = [
      friend({ personId: "supporting-peer-1", nickname: "Rose", handshakeConfirmed: true }),
      friend({ personId: "supporting-peer-2", nickname: "Lane", handshakeConfirmed: true }),
    ];
    const unsupported = friend({
      personId: "unsupported-peer-1",
      nickname: "Noah",
      handshakeConfirmed: false,
    });
    const renderActive = (activePersonId: string) => oslChatsViewMarkup(model({
      friends: [...supported, unsupported],
      activePersonId,
      draft: "TASK0434 direct chat",
    }));
    const supportedMarkup = supported.map((peer) => renderActive(peer.personId));
    const unsupportedMarkup = renderActive(unsupported.personId);
    const supportedEnabled = supportedMarkup.filter((markup) => /class="osl-chat-send" type="submit"(?![^>]* disabled)[^>]*data-osl-chat-peer-state="mutual"/u.test(markup)).length;
    const unsupportedWeaklyBlocked = /class="osl-chat-send" type="submit"[^>]*data-osl-chat-peer-state="one-way"[^>]*disabled/u.test(unsupportedMarkup)
      && unsupportedMarkup.includes("Chat needs both people to answer before sending.");

    console.log(`TASK0434 supported_peers_send_with_stronger_state=${supportedEnabled} state=mutual`);
    console.log(`TASK0434 unsupported_peers_cannot_silently_send_weakly=${unsupportedWeaklyBlocked ? 1 : 0} state=one-way`);
    expect(supportedEnabled).toBe(2);
    expect(unsupportedWeaklyBlocked).toBe(true);
  it("shows the changed-build warning for changed and corrupt proofs while send stays available", () => {
    for (const [reason, marker] of [["changed", "changed"], ["corruptProof", "corrupt-proof"]] as const) {
      const markup = oslChatsViewMarkup(model({
        draft: "Hello",
        buildWarning: {
          kind: "changedBuild",
          reason,
          message: "OSL build changed after its startup proof. Sending stays available.",
          messageSendingAvailable: true,
        },
      }));
      expect(markup).toContain(`data-osl-chat-build-warning="${marker}"`);
      expect(markup).toContain("Changed build warning");
      expect(markup).toContain('data-message-sending-available="true"');
      expect(markup).toMatch(/class="osl-chat-send" type="submit"(?![^>]* disabled)[^>]*>/u);
    }
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

  it("submits the typing box only on a bare Enter", () => {
    expect(submitsOslChatDraft({ key: "Enter" })).toBe(true);
    expect(submitsOslChatDraft({ key: "Enter", shiftKey: true })).toBe(false);
    expect(submitsOslChatDraft({ key: "Enter", ctrlKey: true })).toBe(false);
    expect(submitsOslChatDraft({ key: "Enter", altKey: true })).toBe(false);
    expect(submitsOslChatDraft({ key: "Enter", metaKey: true })).toBe(false);
    expect(submitsOslChatDraft({ key: "Enter", isComposing: true })).toBe(false);
    expect(submitsOslChatDraft({ key: "a" })).toBe(false);
  });

  it("send success clears the live textarea element through the draft source of truth", () => {
    const textarea = { value: "message that was just sent" };
    applyOslChatDraftToElement(textarea, "");
    expect(textarea.value).toBe("");

    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const sendStart = main.indexOf("async function sendOslChat(event: SubmitEvent): Promise<void> {");
    const resetStart = main.indexOf("function resetOslChatUiState", sendStart);
    const prepareStart = main.indexOf("prepareOslChatText(draft, oslChatViewOnce)", sendStart);
    expect(sendStart).toBeGreaterThan(-1);
    expect(resetStart).toBeGreaterThan(sendStart);
    expect(prepareStart).toBeGreaterThan(sendStart);
    expect(main.slice(sendStart, prepareStart)).toContain("oslChatHandshakeConfirmed(oslChatMessages.get(personId) ?? [])");
    expect(main.slice(sendStart, resetStart)).toContain('setOslChatDraft("");');
  });
});
